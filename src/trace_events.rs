use std::collections::HashMap;
use std::io::Write;
use std::time::SystemTime;
use std::vec::Vec;

use failure::Error;
use serde_derive::Serialize;

use crate::stack_trace::StackTrace;

pub struct TraceEvents(pub HashMap<SystemTime, Vec<StackTrace>>);

#[derive(Serialize)]
struct TraceEventData {
    traceEvents: Vec<TraceEvent>,
    stackFrames: HashMap<usize, StackFrame>,
}

#[derive(Serialize)]
struct TraceEvent {
    name: String,
    cat: String,
    ph: String,
    ts: u128,
    pid: u32,
    tid: u64,
    s: String,
    sf: Option<usize>,
}

#[derive(Serialize)]
struct StackFrame {
    name: String,
    category: String,
    parent: Option<usize>,
}

impl TraceEvents {
    pub fn write_trace_events(&self, w: &mut dyn Write) -> Result<(), Error> {
        let mut counter = 0;
        let mut frames = HashMap::new();
        let mut events = Vec::new();

        for (ts, traces) in self.0.iter() {
            let ts = ts.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_micros();
            for trace in traces.iter() {
                let mut parent = None;
                for frame in &trace.frames {
                    let filename = match &frame.short_filename { Some(f) => &f, None => &frame.filename };
                    let name = format!("{} ({}:{})", frame.name, filename, frame.line);
                    parent = match frames.get(&(parent, name.clone())) {
                        Some(frame_id) => Some(*frame_id),
                        None => {
                            let id = counter;
                            frames.insert((parent, name), id);
                            counter += 1;
                            Some(id)
                        }
                    }
                }
                let name = match &trace.thread_name {
                    Some(name) => name.clone(),
                    _ => "".to_string()
                };
                events.push(TraceEvent {
                    name,
                    cat: "".to_string(),
                    ph: "i".to_string(),
                    ts,
                    pid: trace.pid as u32,
                    tid: trace.os_thread_id.unwrap_or(trace.pid as u64),
                    s: "t".to_string(),
                    sf: parent,
                });
            }
        }

        let frames = frames
            .into_iter()
            .map(|((parent, frame), id)| (id, StackFrame {
                name: frame.to_string(),
                category: "".to_string(),
                parent
            }))
            .collect();

        let events = TraceEventData {
            traceEvents: events,
            stackFrames: frames,
        };

        match writeln!(w, "{}", serde_json::to_string(&events)?) {
            Ok(_) => Ok(()),
            Err(error) => Err(Error::from(error)),
        }
    }
}
