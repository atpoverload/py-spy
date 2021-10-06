use std::io::Write;
use std::collections::HashMap;

use failure::Error;

use protobuf::Message;
use protobuf::ProtobufError;

use crate::stack_trace::StackTrace;

use crate::eflect_stack_trace;

// map is indexed by timestamp, thread id, frame id
pub struct TimeSeries {
    traces: HashMap<u128, HashMap<u64, Vec<u64>>>,
    frames: HashMap<String, u64>,
    counter: u64,
}

impl TimeSeries {
    pub fn new() -> TimeSeries {
        TimeSeries { traces: HashMap::new(), frames: HashMap::new(), counter: 0 }
    }

    pub fn add(&mut self, timestamp: u128, trace: &StackTrace) -> std::io::Result<()> {
        for frame in &trace.frames {
            // pull out the filename
            let filename = match &frame.short_filename { Some(f) => &f, None => &frame.filename };
            // pull out the local variables
            let locals = match &frame.locals {
                Some(locals) => locals.iter()
                    .filter(|local| local.arg)
                    .map(|local| {
                        match &local.repr {
                            Some(repr) => format!("{}: {}", &local.name, repr.replace("\"", "")),
                            None => format!("{}", &local.name)
                        }
                    })
                    .collect::<Vec<String>>()
                    .join(", "),
                _ => "".to_string()
            };
            // format as name(locals) (filename:lineno)
            let frame_str = if frame.line != 0 {
                format!("{}({}) ({}:{})", frame.name, locals, filename, frame.line)
            } else if filename.len() > 0 {
                format!("{}({}) ({})", frame.name, locals, filename)
            } else {
                frame.name.clone()
            };
            // add the frame to the mapping so we can just use ints
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
    pub fn write(&self, w: &mut dyn Write) -> Result<(), Error> {
        let mut data_set = eflect_stack_trace::StackTraceDataSet::new();
        self.traces_to_proto().into_iter().for_each(|trace| data_set.samples.push(trace));
        self.frames_to_proto().into_iter().for_each(|frame| data_set.frames.push(frame));
        // self.add_traces(&data_set);
        // self.add_frames(&data_set);
        match data_set.write_to_writer(w) {
            // this is the only possible failure since i filled the proto
            Err(ProtobufError::IoError(err)) => bail!(err),
            _ => Ok(())
        }
    }

    fn traces_to_proto(&self) -> Vec<eflect_stack_trace::StackTraceDataSet_StackTraceSample> {
        let mut samples = Vec::new();
        for (timestamp, data) in self.traces.iter() {
            if data.len() > 0 {
                let mut sample = eflect_stack_trace::StackTraceDataSet_StackTraceSample::new();
                sample.set_timestamp(*timestamp as i64);
                for (thread, frames) in data.iter() {
                    let mut trace = eflect_stack_trace::StackTraceDataSet_ThreadStackTrace::new();
                    trace.set_thread_id(*thread as i64);
                    frames.iter().for_each(|f| trace.frames.push(*f as i64));
                    sample.traces.push(trace);
                }
                samples.push(sample);
            }
        }
        samples
    }

    fn frames_to_proto(&self) -> Vec<eflect_stack_trace::StackTraceDataSet_Frame> {
        self.frames.iter().map(|(frame_name, id)| {
            let mut frame = eflect_stack_trace::StackTraceDataSet_Frame::new();
            frame.set_frame_id(*id as i64);
            frame.set_frame(frame_name.to_string());
            frame
        }).collect()
    }
}
