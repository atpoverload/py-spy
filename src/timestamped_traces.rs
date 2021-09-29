use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use std::collections::HashMap;

use failure::Error;

use crate::sampler;
use crate::stack_trace::{StackTrace, Frame};
use crate::config::Config;

use std::path::Path;

// map is indexed by timestamp, thread id, frame id
pub struct TimestampedTrace {
    traces: HashMap<u128, HashMap<u64, Vec<u64>>>,
    frames: HashMap<String, u64>,
    counter: u64,
}

impl TimestampedTrace {
    pub fn new() -> TimestampedTrace {
        TimestampedTrace { traces: HashMap::new(), frames: HashMap::new(), counter: 0 }
    }

    pub fn add(&mut self, timestamp: u128, trace: &StackTrace) -> std::io::Result<()> {
        for frame in &trace.frames {
            let filename = match &frame.short_filename { Some(f) => &f, None => &frame.filename };
            let locals = match &frame.locals {
                Some(locals) => locals.iter()
                    .filter(|local| local.arg)
                    .map(|local| {
                        match &local.repr {
                            Some(repr) => {
                                format!(
                                    "{}: {}",
                                    &local.name,
                                    repr.replace("\"", "")
                                        .replace("\'", "")
                                        .replace("\\n", "")
                                        .replace("\\", ""))
                            },
                            None => format!("{}", &local.name)
                        }
                    })
                    .collect::<Vec<String>>()
                    .join(", "),
                _ => "".to_string()
            };
            let frame_str = if frame.line != 0 {
                format!("{}({}) ({}:{})", frame.name, locals, filename, frame.line)
            } else if filename.len() > 0 {
                format!("{}({}) ({})", frame.name, locals, filename)
            } else {
                frame.name.clone()
            };
            let frame_id = if !self.frames.contains_key(&frame_str) {
                self.counter += 1;
                self.frames.entry(frame_str).or_insert(self.counter);
                self.counter
            } else {
                *self.frames.get(&frame_str).unwrap()
            };
            self.traces
                .entry(timestamp)
                .or_insert_with(HashMap::new)
                .entry(trace.os_thread_id.unwrap())
                .or_insert_with(Vec::new)
                .push(frame_id);
        }
        Ok(())
    }

    // temporarily writing these as jsons
    pub fn write(&self, filename: &str) -> Result<(), Error> {
        let mut out_file = std::fs::File::create(Path::new(&filename).join("traces.json").to_str().unwrap())?;
        self.write_traces(&mut out_file)?;
        let mut out_file = std::fs::File::create(Path::new(&filename).join("frames.json").to_str().unwrap())?;
        self.write_frames(&mut out_file)?;
        Ok(())
    }

    fn write_traces(&self, w: &mut dyn Write) -> Result<(), Error> {
        let mut frames = Vec::new();
        frames.push("{".to_string());
        let mut counter = 0;
        for (timestamp, data) in self.traces.iter() {
            if data.len() > 0 {
                frames.push(format!("\t\"{}\": {{", timestamp));
                let mut inner_counter = 0;
                for (thread, frame) in data.iter() {
                    inner_counter += 1;
                    if inner_counter < data.len() {
                        frames.push(format!("\t\t\"{}\": {:?},", thread, frame));
                    } else {
                        frames.push(format!("\t\t\"{}\": {:?}", thread, frame));
                    }
                }

                counter += 1;
                if counter < self.traces.len() {
                    frames.push("\t},".to_string());
                } else {
                    frames.push("\t}".to_string());
                }
            }
        }
        frames.push("}".to_string());
        w.write_all(frames.join("\n").as_bytes())?;
        Ok(())
    }

    fn write_frames(&self, w: &mut dyn Write) -> Result<(), Error> {
        let mut frames = Vec::new();
        frames.push("{".to_string());
        let mut counter = 0;
        for (frame, id) in self.frames.iter() {
            counter += 1;
            if counter < self.frames.len() {
                frames.push(format!("\"{:?}\":{:?},", id, frame));
            } else {
                frames.push(format!("\"{:?}\":{:?}", id, frame));
            }
        }
        frames.push("}".to_string());
        w.write_all(frames.join("\n").as_bytes())?;
        Ok(())
    }
}

pub fn record_samples(pid: remoteprocess::Pid, config: &Config) -> Result<(), Error> {
    let filename = match config.filename.clone() {
        Some(filename) => filename,
        None => return Err(format_err!("A directory is required to record samples"))
    };

    let sampler = sampler::Sampler::new(pid, config)?;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    let mut data = TimestampedTrace::new();
    for mut sample in sampler {
        if !running.load(Ordering::SeqCst) {
            break;
        }

        for trace in sample.traces.iter_mut() {
            if !(config.include_idle || trace.active) {
                continue;
            }

            if config.gil_only && !trace.owns_gil {
                continue;
            }

            if config.include_thread_ids {
                let threadid = trace.format_threadid();
                trace.frames.push(Frame{name: format!("thread ({})", threadid),
                    filename: String::from(""),
                    module: None, short_filename: None, line: 0, locals: None});
            }

            if let Some(process_info) = trace.process_info.as_ref().map(|x| x) {
                trace.frames.push(process_info.to_frame());
                let mut parent = process_info.parent.as_ref();
                while parent.is_some() {
                    if let Some(process_info) = parent {
                        trace.frames.push(process_info.to_frame());
                        parent = process_info.parent.as_ref();
                    }
                }
            }

            if let Some(_) = trace.os_thread_id {
                data.add(
                    SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis(),
                    trace)?;
            }
        }
    }

    data.write(&filename)
}
