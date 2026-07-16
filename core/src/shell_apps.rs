//! Phase 5 first-application and shell-screen proof.
//!
//! The first-party shell applications are fixed, capability-scoped services
//! with typed windows. The shell screen is composed through the compositor
//! surface path instead of bypassing the Phase 5 presentation model.

#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::{
    capabilities::{CapabilityError, CapabilityHandle, CapabilityTable, ResourceId, RightsMask},
    compositor::{CompositorError, FramebufferTarget, Surface},
    service_identity::{ServiceId, ServiceIdentityError, ServiceIdentityTable},
    shell_objects::{DrawableObject, ObjectId, ObjectKind, PresentationBinding, ShellObjectError},
    tasks::TaskId,
};

const APP_COUNT: usize = 4;
const SHELL_WINDOW_RIGHTS: RightsMask = RightsMask::new(RightsMask::READ);

const LAUNCHER_COLOR: u32 = 0x0000_4488;
const SERVICE_MONITOR_COLOR: u32 = 0x0000_8844;
const PYTHON_CONSOLE_COLOR: u32 = 0x0088_4400;
const SETTINGS_PANEL_COLOR: u32 = 0x0044_0088;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellAppError {
    Capability(CapabilityError),
    ServiceIdentity(ServiceIdentityError),
    ShellObject(ShellObjectError),
    Compositor(CompositorError),
    MissingApp,
    BadRegistration,
    BadRender,
}

impl From<CapabilityError> for ShellAppError {
    fn from(error: CapabilityError) -> Self {
        Self::Capability(error)
    }
}

impl From<ServiceIdentityError> for ShellAppError {
    fn from(error: ServiceIdentityError) -> Self {
        Self::ServiceIdentity(error)
    }
}

impl From<ShellObjectError> for ShellAppError {
    fn from(error: ShellObjectError) -> Self {
        Self::ShellObject(error)
    }
}

impl From<CompositorError> for ShellAppError {
    fn from(error: CompositorError) -> Self {
        Self::Compositor(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellAppKind {
    Launcher,
    ServiceMonitor,
    PythonConsole,
    SettingsPanel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShellApplication {
    kind: ShellAppKind,
    service: ServiceId,
    resource: ResourceId,
    capability: CapabilityHandle,
    window: DrawableObject,
    binding: PresentationBinding,
}

impl ShellApplication {
    pub const fn kind(self) -> ShellAppKind {
        self.kind
    }

    pub const fn service(self) -> ServiceId {
        self.service
    }

    pub const fn resource(self) -> ResourceId {
        self.resource
    }

    pub const fn capability(self) -> CapabilityHandle {
        self.capability
    }

    pub const fn window(self) -> DrawableObject {
        self.window
    }

    pub const fn binding(self) -> PresentationBinding {
        self.binding
    }
}

pub struct ShellApplicationSet {
    apps: [ShellApplication; APP_COUNT],
}

impl ShellApplicationSet {
    pub const fn new(apps: [ShellApplication; APP_COUNT]) -> Self {
        Self { apps }
    }

    pub fn app(&self, kind: ShellAppKind) -> Option<ShellApplication> {
        self.apps.iter().copied().find(|app| app.kind() == kind)
    }
}

#[derive(Clone, Copy)]
struct AppSpec {
    task: TaskId,
    kind: ShellAppKind,
    resource: ResourceId,
    object_id: ObjectId,
    object_kind: ObjectKind,
    binding: BindingSpec,
}

#[derive(Clone, Copy)]
struct BindingSpec {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    z_order: u8,
}

fn register_app(
    identities: &mut ServiceIdentityTable,
    capabilities: &mut CapabilityTable,
    spec: AppSpec,
) -> Result<ShellApplication, ShellAppError> {
    let service = identities.register_task(spec.task)?;
    let capability = capabilities.grant(service, spec.resource, SHELL_WINDOW_RIGHTS)?;
    capabilities.validate(service, capability, spec.resource, SHELL_WINDOW_RIGHTS)?;
    let window = DrawableObject::new(spec.object_id, spec.object_kind);
    let binding = PresentationBinding::new(
        window,
        spec.binding.x,
        spec.binding.y,
        spec.binding.width,
        spec.binding.height,
        spec.binding.z_order,
    )?;
    Ok(ShellApplication {
        kind: spec.kind,
        service,
        resource: spec.resource,
        capability,
        window,
        binding,
    })
}

pub fn register_first_party_apps() -> Result<ShellApplicationSet, ShellAppError> {
    let mut identities = ServiceIdentityTable::new();
    let mut capabilities = CapabilityTable::new();
    let launcher = register_app(
        &mut identities,
        &mut capabilities,
        AppSpec {
            task: TaskId::new(120),
            kind: ShellAppKind::Launcher,
            resource: ResourceId::new(0x7201),
            object_id: ObjectId::new(0x7201),
            object_kind: ObjectKind::ApplicationLauncherWindow,
            binding: BindingSpec {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
                z_order: 0,
            },
        },
    )?;
    let service_monitor = register_app(
        &mut identities,
        &mut capabilities,
        AppSpec {
            task: TaskId::new(121),
            kind: ShellAppKind::ServiceMonitor,
            resource: ResourceId::new(0x7202),
            object_id: ObjectId::new(0x7202),
            object_kind: ObjectKind::ServiceMonitorWindow,
            binding: BindingSpec {
                x: 2,
                y: 0,
                width: 2,
                height: 2,
                z_order: 1,
            },
        },
    )?;
    let python_console = register_app(
        &mut identities,
        &mut capabilities,
        AppSpec {
            task: TaskId::new(122),
            kind: ShellAppKind::PythonConsole,
            resource: ResourceId::new(0x7203),
            object_id: ObjectId::new(0x7203),
            object_kind: ObjectKind::PythonConsoleWindow,
            binding: BindingSpec {
                x: 0,
                y: 2,
                width: 4,
                height: 2,
                z_order: 2,
            },
        },
    )?;
    let settings_panel = register_app(
        &mut identities,
        &mut capabilities,
        AppSpec {
            task: TaskId::new(123),
            kind: ShellAppKind::SettingsPanel,
            resource: ResourceId::new(0x7204),
            object_id: ObjectId::new(0x7204),
            object_kind: ObjectKind::SettingsPanelWindow,
            binding: BindingSpec {
                x: 4,
                y: 2,
                width: 4,
                height: 2,
                z_order: 3,
            },
        },
    )?;
    Ok(ShellApplicationSet::new([
        launcher,
        service_monitor,
        python_console,
        settings_panel,
    ]))
}

pub fn render_shell_screen(apps: &ShellApplicationSet) -> Result<[u32; 32], ShellAppError> {
    let mut pixels = [0u32; 32];
    {
        let mut target = FramebufferTarget::new(&mut pixels, 8, 4, 8)?;
        let launcher = apps
            .app(ShellAppKind::Launcher)
            .ok_or(ShellAppError::MissingApp)?;
        let launcher_pixels = [LAUNCHER_COLOR; 4];
        let launcher_surface = Surface::new(
            launcher.window(),
            launcher.binding(),
            &launcher_pixels,
            2,
            2,
            2,
        )?;
        target.compose(&launcher_surface)?;

        let service_monitor = apps
            .app(ShellAppKind::ServiceMonitor)
            .ok_or(ShellAppError::MissingApp)?;
        let service_pixels = [SERVICE_MONITOR_COLOR; 4];
        let service_surface = Surface::new(
            service_monitor.window(),
            service_monitor.binding(),
            &service_pixels,
            2,
            2,
            2,
        )?;
        target.compose(&service_surface)?;

        let python_console = apps
            .app(ShellAppKind::PythonConsole)
            .ok_or(ShellAppError::MissingApp)?;
        let python_pixels = [PYTHON_CONSOLE_COLOR; 8];
        let python_surface = Surface::new(
            python_console.window(),
            python_console.binding(),
            &python_pixels,
            4,
            2,
            4,
        )?;
        target.compose(&python_surface)?;

        let settings_panel = apps
            .app(ShellAppKind::SettingsPanel)
            .ok_or(ShellAppError::MissingApp)?;
        let settings_pixels = [SETTINGS_PANEL_COLOR; 8];
        let settings_surface = Surface::new(
            settings_panel.window(),
            settings_panel.binding(),
            &settings_pixels,
            4,
            2,
            4,
        )?;
        target.compose(&settings_surface)?;
    }
    Ok(pixels)
}

pub fn run_self_test() -> Result<(), ShellAppError> {
    let apps = register_first_party_apps()?;
    let launcher = apps
        .app(ShellAppKind::Launcher)
        .ok_or(ShellAppError::BadRegistration)?;
    let service_monitor = apps
        .app(ShellAppKind::ServiceMonitor)
        .ok_or(ShellAppError::BadRegistration)?;
    let python_console = apps
        .app(ShellAppKind::PythonConsole)
        .ok_or(ShellAppError::BadRegistration)?;
    let settings_panel = apps
        .app(ShellAppKind::SettingsPanel)
        .ok_or(ShellAppError::BadRegistration)?;

    if launcher.service() == service_monitor.service()
        || launcher.resource() == service_monitor.resource()
        || launcher.capability() == service_monitor.capability()
    {
        return Err(ShellAppError::BadRegistration);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:APP:LAUNCHER");

    if service_monitor.service() == python_console.service()
        || service_monitor.resource() == python_console.resource()
        || service_monitor.capability() == python_console.capability()
    {
        return Err(ShellAppError::BadRegistration);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:APP:SERVICE_MONITOR");

    if python_console.service() == settings_panel.service()
        || python_console.resource() == settings_panel.resource()
        || python_console.capability() == settings_panel.capability()
    {
        return Err(ShellAppError::BadRegistration);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:APP:PYTHON_CONSOLE");

    if settings_panel.window().kind() != ObjectKind::SettingsPanelWindow {
        return Err(ShellAppError::BadRegistration);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:APP:SETTINGS_PANEL");

    let pixels = render_shell_screen(&apps)?;
    let expected = [
        LAUNCHER_COLOR,
        LAUNCHER_COLOR,
        SERVICE_MONITOR_COLOR,
        SERVICE_MONITOR_COLOR,
        0,
        0,
        0,
        0,
        LAUNCHER_COLOR,
        LAUNCHER_COLOR,
        SERVICE_MONITOR_COLOR,
        SERVICE_MONITOR_COLOR,
        0,
        0,
        0,
        0,
        PYTHON_CONSOLE_COLOR,
        PYTHON_CONSOLE_COLOR,
        PYTHON_CONSOLE_COLOR,
        PYTHON_CONSOLE_COLOR,
        SETTINGS_PANEL_COLOR,
        SETTINGS_PANEL_COLOR,
        SETTINGS_PANEL_COLOR,
        SETTINGS_PANEL_COLOR,
        PYTHON_CONSOLE_COLOR,
        PYTHON_CONSOLE_COLOR,
        PYTHON_CONSOLE_COLOR,
        PYTHON_CONSOLE_COLOR,
        SETTINGS_PANEL_COLOR,
        SETTINGS_PANEL_COLOR,
        SETTINGS_PANEL_COLOR,
        SETTINGS_PANEL_COLOR,
    ];
    if pixels != expected {
        return Err(ShellAppError::BadRender);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_party_apps_have_capability_scoped_services_and_typed_windows() {
        let apps = register_first_party_apps().unwrap();
        let launcher = apps.app(ShellAppKind::Launcher).unwrap();
        let service_monitor = apps.app(ShellAppKind::ServiceMonitor).unwrap();
        let python_console = apps.app(ShellAppKind::PythonConsole).unwrap();
        let settings_panel = apps.app(ShellAppKind::SettingsPanel).unwrap();

        assert_eq!(
            launcher.window().kind(),
            ObjectKind::ApplicationLauncherWindow
        );
        assert_eq!(
            service_monitor.window().kind(),
            ObjectKind::ServiceMonitorWindow
        );
        assert_eq!(
            python_console.window().kind(),
            ObjectKind::PythonConsoleWindow
        );
        assert_eq!(
            settings_panel.window().kind(),
            ObjectKind::SettingsPanelWindow
        );
        assert_ne!(launcher.service(), service_monitor.service());
        assert_ne!(launcher.capability(), service_monitor.capability());
        assert_ne!(launcher.resource(), service_monitor.resource());
    }

    #[test]
    fn shell_screen_composes_first_party_app_windows() {
        let apps = register_first_party_apps().unwrap();
        let pixels = render_shell_screen(&apps).unwrap();

        assert_eq!(pixels[0], LAUNCHER_COLOR);
        assert_eq!(pixels[2], SERVICE_MONITOR_COLOR);
        assert_eq!(pixels[16], PYTHON_CONSOLE_COLOR);
        assert_eq!(pixels[20], SETTINGS_PANEL_COLOR);
    }
}
