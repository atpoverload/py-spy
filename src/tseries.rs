use std::collections::HashMap;
use std::io::Write;

use failure::Error;

use serde_json;

use crate::stack_trace::StackTrace;

pub struct TimeSeries {
    traces: HashMap<u128, Vec<StackTrace>>,
}

impl TimeSeries {
    pub fn new() -> TimeSeries {
        TimeSeries {
            traces: HashMap::new(),
        }
    }

    pub fn add(&mut self, timestamp: u128, trace: &StackTrace) -> std::io::Result<()> {
        self.traces
            .entry(timestamp)
            .or_insert_with(Vec::new)
            .push(trace.clone());
        Ok(())
    }

    pub fn write(&self, w: &mut dyn Write) -> Result<(), Error> {
        match writeln!(w, "{}", serde_json::to_string(&self.traces)?) {
            Ok(_) => Ok(()),
            Err(error) => Err(Error::from(error)),
        }
    }
}
