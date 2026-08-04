use anyhow::Result;
use serde::Serialize;

pub(crate) struct Output {
    json: bool,
}

impl Output {
    pub(crate) fn new(json: bool) -> Self {
        Self { json }
    }

    pub(crate) fn success<T: Serialize>(
        &self,
        status: &'static str,
        result: T,
        human: String,
    ) -> Result<()> {
        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&MachineResult { status, result })?
            );
        } else {
            println!("{human}");
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct MachineResult<T> {
    status: &'static str,
    result: T,
}
