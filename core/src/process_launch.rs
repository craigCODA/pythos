#![cfg_attr(test, allow(dead_code))]

use crate::capabilities::{CapabilityTable, ResourceId, RightsMask};
use crate::dynamic_capabilities::{
    CreatorGrantPolicy, DynamicCapabilityError, DynamicProcess, DynamicProcessTable, InitialGrant,
};
use crate::service_identity::ServiceIdentityTable;
use crate::tasks::TaskId;

const MAX_LAUNCH_STRINGS: usize = 4;
const MAX_LAUNCH_STRING_BYTES: usize = 32;
const ENV_SECRET_RESOURCE: ResourceId = ResourceId::new(0x4152_4756_454E_5601);
const ENV_READ_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessLaunchError {
    EmptyString,
    StringTooLong,
    InvalidUtf8,
    ArgvFull,
    EnvironmentFull,
    DuplicateEnvironmentKey,
    UnknownEnvironmentKey,
    EnvironmentCapabilityDenied,
    DynamicCapability(DynamicCapabilityError),
    ProofFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessLaunchProof {
    pub argv_delivered: bool,
    pub env_capability_allowed: bool,
    pub ungranted_env_denied: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LaunchString {
    bytes: [u8; MAX_LAUNCH_STRING_BYTES],
    len: usize,
}

impl LaunchString {
    pub fn new(value: &str) -> Result<Self, ProcessLaunchError> {
        let bytes = value.as_bytes();
        if bytes.is_empty() {
            return Err(ProcessLaunchError::EmptyString);
        }
        if bytes.len() > MAX_LAUNCH_STRING_BYTES {
            return Err(ProcessLaunchError::StringTooLong);
        }

        let mut stored = [0; MAX_LAUNCH_STRING_BYTES];
        stored[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            bytes: stored,
            len: bytes.len(),
        })
    }

    pub fn as_str(&self) -> Result<&str, ProcessLaunchError> {
        core::str::from_utf8(&self.bytes[..self.len]).map_err(|_| ProcessLaunchError::InvalidUtf8)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LaunchArguments {
    args: [Option<LaunchString>; MAX_LAUNCH_STRINGS],
    count: usize,
}

impl LaunchArguments {
    pub const fn new() -> Self {
        Self {
            args: [None; MAX_LAUNCH_STRINGS],
            count: 0,
        }
    }

    pub fn push(&mut self, argument: LaunchString) -> Result<(), ProcessLaunchError> {
        if self.count >= MAX_LAUNCH_STRINGS {
            return Err(ProcessLaunchError::ArgvFull);
        }
        self.args[self.count] = Some(argument);
        self.count += 1;
        Ok(())
    }

    pub const fn count(&self) -> usize {
        self.count
    }

    pub const fn get(&self, index: usize) -> Option<LaunchString> {
        if index < self.count {
            self.args[index]
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnvironmentEntry {
    key: LaunchString,
    value: LaunchString,
    resource: ResourceId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessEnvironment {
    entries: [Option<EnvironmentEntry>; MAX_LAUNCH_STRINGS],
    count: usize,
}

impl ProcessEnvironment {
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_LAUNCH_STRINGS],
            count: 0,
        }
    }

    pub fn insert(
        &mut self,
        key: LaunchString,
        value: LaunchString,
        resource: ResourceId,
    ) -> Result<(), ProcessLaunchError> {
        if self.count >= MAX_LAUNCH_STRINGS {
            return Err(ProcessLaunchError::EnvironmentFull);
        }
        if self.entries[..self.count]
            .iter()
            .flatten()
            .any(|entry| entry.key == key)
        {
            return Err(ProcessLaunchError::DuplicateEnvironmentKey);
        }
        self.entries[self.count] = Some(EnvironmentEntry {
            key,
            value,
            resource,
        });
        self.count += 1;
        Ok(())
    }

    fn find(&self, key: LaunchString) -> Option<EnvironmentEntry> {
        let mut index = 0;
        while index < self.count {
            if let Some(entry) = self.entries[index]
                && entry.key == key
            {
                return Some(entry);
            }
            index += 1;
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessLaunchContext {
    process: DynamicProcess,
    argv: LaunchArguments,
    environment: ProcessEnvironment,
}

impl ProcessLaunchContext {
    pub const fn new(
        process: DynamicProcess,
        argv: LaunchArguments,
        environment: ProcessEnvironment,
    ) -> Self {
        Self {
            process,
            argv,
            environment,
        }
    }

    pub const fn argv(&self) -> &LaunchArguments {
        &self.argv
    }

    pub fn read_env(
        &self,
        capabilities: &CapabilityTable,
        key: LaunchString,
    ) -> Result<LaunchString, ProcessLaunchError> {
        let entry = self
            .environment
            .find(key)
            .ok_or(ProcessLaunchError::UnknownEnvironmentKey)?;
        self.process
            .use_capability(capabilities, entry.resource, ENV_READ_RIGHTS)
            .map_err(|_| ProcessLaunchError::EnvironmentCapabilityDenied)?;
        Ok(entry.value)
    }
}

pub fn run_self_test() -> Result<ProcessLaunchProof, ProcessLaunchError> {
    let mut identities = ServiceIdentityTable::new();
    let mut capabilities = CapabilityTable::new();
    let mut processes = DynamicProcessTable::new();
    let mut argv = LaunchArguments::new();
    argv.push(LaunchString::new("user0.elf")?)?;
    argv.push(LaunchString::new("--proof")?)?;
    let mut environment = ProcessEnvironment::new();
    environment.insert(
        LaunchString::new("PYTHOS_SECRET")?,
        LaunchString::new("visible")?,
        ENV_SECRET_RESOURCE,
    )?;

    let denied_process = processes
        .spawn(
            &mut identities,
            &mut capabilities,
            TaskId::new(150),
            CreatorGrantPolicy::empty(),
        )
        .map_err(ProcessLaunchError::DynamicCapability)?;
    let denied_context = ProcessLaunchContext::new(denied_process, argv, environment);
    let first_arg_matches = denied_context
        .argv()
        .get(0)
        .map(|arg| arg.as_str() == Ok("user0.elf"))
        .unwrap_or(false);
    let second_arg_matches = denied_context
        .argv()
        .get(1)
        .map(|arg| arg.as_str() == Ok("--proof"))
        .unwrap_or(false);
    let argv_delivered =
        denied_context.argv().count() == 2 && first_arg_matches && second_arg_matches;
    let ungranted_env_denied = denied_context
        .read_env(&capabilities, LaunchString::new("PYTHOS_SECRET")?)
        == Err(ProcessLaunchError::EnvironmentCapabilityDenied);

    let granted_process = processes
        .spawn(
            &mut identities,
            &mut capabilities,
            TaskId::new(151),
            CreatorGrantPolicy::single(InitialGrant::new(ENV_SECRET_RESOURCE, ENV_READ_RIGHTS)),
        )
        .map_err(ProcessLaunchError::DynamicCapability)?;
    let granted_context = ProcessLaunchContext::new(granted_process, argv, environment);
    let env_capability_allowed = granted_context
        .read_env(&capabilities, LaunchString::new("PYTHOS_SECRET")?)
        .and_then(|value| {
            if value.as_str()? == "visible" {
                Ok(value)
            } else {
                Err(ProcessLaunchError::ProofFailed)
            }
        })
        .is_ok();

    if !argv_delivered || !env_capability_allowed || !ungranted_env_denied {
        return Err(ProcessLaunchError::ProofFailed);
    }

    Ok(ProcessLaunchProof {
        argv_delivered,
        env_capability_allowed,
        ungranted_env_denied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::{CapabilityTable, ResourceId, RightsMask};
    use crate::dynamic_capabilities::{CreatorGrantPolicy, DynamicProcessTable, InitialGrant};
    use crate::service_identity::ServiceIdentityTable;
    use crate::tasks::TaskId;

    const ENV_SECRET_RESOURCE: ResourceId = ResourceId::new(0x4152_4756_454E_5601);
    const ENV_READ_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ);

    #[test]
    fn launch_strings_are_bounded_and_nonempty() {
        assert_eq!(LaunchString::new(""), Err(ProcessLaunchError::EmptyString));
        assert_eq!(
            LaunchString::new("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz"),
            Err(ProcessLaunchError::StringTooLong)
        );

        assert_eq!(
            LaunchString::new("user0.elf").unwrap().as_str().unwrap(),
            "user0.elf"
        );
    }

    #[test]
    fn argv_is_delivered_to_launch_context() {
        let mut identities = ServiceIdentityTable::new();
        let mut capabilities = CapabilityTable::new();
        let mut processes = DynamicProcessTable::new();
        let process = processes
            .spawn(
                &mut identities,
                &mut capabilities,
                TaskId::new(150),
                CreatorGrantPolicy::empty(),
            )
            .unwrap();
        let mut argv = LaunchArguments::new();
        argv.push(LaunchString::new("user0.elf").unwrap()).unwrap();
        argv.push(LaunchString::new("--proof").unwrap()).unwrap();

        let context = ProcessLaunchContext::new(process, argv, ProcessEnvironment::new());

        assert_eq!(context.argv().count(), 2);
        assert_eq!(
            context.argv().get(0).unwrap().as_str().unwrap(),
            "user0.elf"
        );
        assert_eq!(context.argv().get(1).unwrap().as_str().unwrap(), "--proof");
    }

    #[test]
    fn environment_read_requires_matching_capability() {
        let mut identities = ServiceIdentityTable::new();
        let mut capabilities = CapabilityTable::new();
        let mut processes = DynamicProcessTable::new();
        let process = processes
            .spawn(
                &mut identities,
                &mut capabilities,
                TaskId::new(151),
                CreatorGrantPolicy::single(InitialGrant::new(ENV_SECRET_RESOURCE, ENV_READ_RIGHTS)),
            )
            .unwrap();
        let mut environment = ProcessEnvironment::new();
        environment
            .insert(
                LaunchString::new("PYTHOS_SECRET").unwrap(),
                LaunchString::new("visible").unwrap(),
                ENV_SECRET_RESOURCE,
            )
            .unwrap();
        let context = ProcessLaunchContext::new(process, LaunchArguments::new(), environment);

        assert_eq!(
            context
                .read_env(&capabilities, LaunchString::new("PYTHOS_SECRET").unwrap())
                .unwrap()
                .as_str()
                .unwrap(),
            "visible"
        );
    }

    #[test]
    fn ungranted_environment_key_is_denied_even_when_known() {
        let mut identities = ServiceIdentityTable::new();
        let mut capabilities = CapabilityTable::new();
        let mut processes = DynamicProcessTable::new();
        let process = processes
            .spawn(
                &mut identities,
                &mut capabilities,
                TaskId::new(152),
                CreatorGrantPolicy::empty(),
            )
            .unwrap();
        let mut environment = ProcessEnvironment::new();
        environment
            .insert(
                LaunchString::new("PYTHOS_SECRET").unwrap(),
                LaunchString::new("hidden").unwrap(),
                ENV_SECRET_RESOURCE,
            )
            .unwrap();
        let context = ProcessLaunchContext::new(process, LaunchArguments::new(), environment);

        assert_eq!(
            context.read_env(&capabilities, LaunchString::new("PYTHOS_SECRET").unwrap()),
            Err(ProcessLaunchError::EnvironmentCapabilityDenied)
        );
    }

    #[test]
    fn unknown_environment_key_is_distinct_from_capability_denial() {
        let mut identities = ServiceIdentityTable::new();
        let mut capabilities = CapabilityTable::new();
        let mut processes = DynamicProcessTable::new();
        let process = processes
            .spawn(
                &mut identities,
                &mut capabilities,
                TaskId::new(153),
                CreatorGrantPolicy::single(InitialGrant::new(ENV_SECRET_RESOURCE, ENV_READ_RIGHTS)),
            )
            .unwrap();
        let context =
            ProcessLaunchContext::new(process, LaunchArguments::new(), ProcessEnvironment::new());

        assert_eq!(
            context.read_env(&capabilities, LaunchString::new("PYTHOS_SECRET").unwrap()),
            Err(ProcessLaunchError::UnknownEnvironmentKey)
        );
    }

    #[test]
    fn self_test_proves_required_launch_context_cases() {
        let proof = run_self_test().unwrap();

        assert!(proof.argv_delivered);
        assert!(proof.env_capability_allowed);
        assert!(proof.ungranted_env_denied);
    }
}
