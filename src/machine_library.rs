//! Portable machine definitions with their intended postprocessor, never live connections.
use crate::{
    machine::SetupProfile,
    model::Project,
    post::{PostProcessor, PostSettings},
    process::Process,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineRecord {
    pub name: String,
    pub profile: SetupProfile,
    pub post: PostSettings,
}
impl MachineRecord {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.len() > 200 {
            return Err("Machine name must be 1–200 characters".into());
        }
        self.post.validate(self.profile.process)?;
        let mut p = Project::new_for_process(self.profile.process);
        self.profile.apply(&mut p)?;
        p.post = self.post.clone();
        crate::cam::validate_setup(&p)
    }
    pub fn from_project(p: &Project) -> Result<Self, String> {
        let record = Self {
            name: p
                .machine
                .as_ref()
                .ok_or("Choose a machine first")?
                .name
                .clone(),
            profile: SetupProfile::from_project(p)?,
            post: p.post.clone(),
        };
        record.validate()?;
        Ok(record)
    }
    pub fn apply(&self, p: &mut Project) -> Result<(), String> {
        self.validate()?;
        apply_profile(&self.profile, p)?;
        p.post = self.post.clone();
        Ok(())
    }
}
pub fn apply_profile(profile: &SetupProfile, p: &mut Project) -> Result<(), String> {
    if profile.format_version == 1 && !p.tool_library.tools.is_empty() && p.active_tool.is_none() {
        let mut temp = Project::new_for_process(p.process);
        profile.apply(&mut temp)?;
        p.machine = temp.machine;
        p.tool = temp.tool;
        Ok(())
    } else {
        profile.apply(p)
    }
}
pub fn builtins() -> Vec<MachineRecord> {
    [
        PostProcessor::Generic,
        PostProcessor::Grbl,
        PostProcessor::Mach3,
        PostProcessor::LinuxCnc,
    ]
    .into_iter()
    .map(|post| {
        let mut p = Project::new_for_process(Process::Milling);
        let machine = p.machine.as_mut().unwrap();
        machine.name = format!("Generic 3-axis mill · {}", post.label());
        machine.configuration.controller = post.label().into();
        p.post.processor = post;
        MachineRecord::from_project(&p).unwrap()
    })
    .collect()
}
pub fn parse(data: &str) -> Result<Vec<MachineRecord>, String> {
    if data.len() > 8 * 1024 * 1024 {
        return Err("Machine library exceeds 8 MiB".into());
    }
    let value: serde_json::Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
    let records = if let Some(machines) = value.get("machines") {
        if value.get("format_version").and_then(|v| v.as_u64()) != Some(1) {
            return Err("Unsupported machine library version".into());
        }
        serde_json::from_value(machines.clone()).map_err(|e| e.to_string())?
    } else if value.get("profile").is_some() {
        vec![serde_json::from_value(value).map_err(|e| e.to_string())?]
    } else {
        let profile: SetupProfile = serde_json::from_value(value).map_err(|e| e.to_string())?;
        vec![MachineRecord {
            name: profile.machine.name.clone(),
            post: PostSettings::for_process(profile.process),
            profile,
        }]
    };
    let records: Vec<MachineRecord> = records;
    if records.is_empty() || records.len() > 200 {
        return Err("Machine library must contain 1–200 definitions".into());
    }
    for record in &records {
        record.validate()?
    }
    Ok(records)
}
