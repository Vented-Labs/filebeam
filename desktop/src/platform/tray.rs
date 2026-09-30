#[cfg(target_os = "linux")]
use std::sync::mpsc::{self, Receiver};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresenceEvent {
    Show,
    Quit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresenceAvailability {
    pub available: bool,
    pub reason: Option<String>,
}

pub struct NativePresence {
    availability: PresenceAvailability,
    backend: Backend,
}

impl NativePresence {
    /// Construct this from the GPUI/UI thread. The macOS and Windows backends retain native UI
    /// objects and must not be moved to a worker.
    pub fn new() -> Result<Self, String> {
        Backend::new().map(|(availability, backend)| Self {
            availability,
            backend,
        })
    }

    pub fn availability(&self) -> &PresenceAvailability {
        &self.availability
    }

    pub fn try_event(&self) -> Option<PresenceEvent> {
        self.backend.try_event()
    }
}

#[cfg(target_os = "linux")]
enum Backend {
    Active {
        events: Receiver<PresenceEvent>,
        _handle: ksni::blocking::Handle<LinuxTray>,
    },
    Unavailable,
}

#[cfg(target_os = "linux")]
impl Backend {
    fn new() -> Result<(PresenceAvailability, Self), String> {
        use ksni::blocking::TrayMethods;
        let (sender, events) = mpsc::channel();
        let icon = linux_icon()?;
        let handle = match (LinuxTray { sender, icon }).spawn() {
            Ok(handle) => handle,
            Err(_) => return Ok(unavailable("no StatusNotifierItem watcher is available")),
        };
        Ok((
            PresenceAvailability {
                available: true,
                reason: None,
            },
            Self::Active {
                events,
                _handle: handle,
            },
        ))
    }

    fn try_event(&self) -> Option<PresenceEvent> {
        match self {
            Self::Active { events, .. } => events.try_recv().ok(),
            Self::Unavailable => None,
        }
    }
}

#[cfg(target_os = "linux")]
fn unavailable(reason: &str) -> (PresenceAvailability, Backend) {
    (
        PresenceAvailability {
            available: false,
            reason: Some(reason.into()),
        },
        Backend::Unavailable,
    )
}

#[cfg(target_os = "linux")]
struct LinuxTray {
    sender: mpsc::Sender<PresenceEvent>,
    icon: ksni::Icon,
}

#[cfg(target_os = "linux")]
impl ksni::Tray for LinuxTray {
    fn id(&self) -> String {
        "filebeam".into()
    }
    fn title(&self) -> String {
        "Filebeam".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.icon.clone()]
    }
    fn activate(&mut self, _: i32, _: i32) {
        let _ = self.sender.send(PresenceEvent::Show);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let show = self.sender.clone();
        let quit = self.sender.clone();
        vec![
            ksni::menu::StandardItem {
                label: "Show Filebeam".into(),
                activate: Box::new(move |_| {
                    let _ = show.send(PresenceEvent::Show);
                }),
                ..Default::default()
            }
            .into(),
            ksni::menu::StandardItem {
                label: "Quit Filebeam".into(),
                activate: Box::new(move |_| {
                    let _ = quit.send(PresenceEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

#[cfg(target_os = "linux")]
fn linux_icon() -> Result<ksni::Icon, String> {
    let image = image::load_from_memory(include_bytes!("../../../backend/public/icon-192.png"))
        .map_err(|error| format!("decode Filebeam tray icon: {error}"))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let mut argb = Vec::with_capacity((width * height * 4) as usize);
    for &[red, green, blue, alpha] in image.as_raw().as_chunks::<4>().0 {
        argb.extend_from_slice(&[alpha, red, green, blue]);
    }
    Ok(ksni::Icon {
        width: width as i32,
        height: height as i32,
        data: argb,
    })
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
enum Backend {
    Active {
        tray: tray_icon::TrayIcon,
        show: tray_icon::menu::MenuItem,
        quit: tray_icon::menu::MenuItem,
    },
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Backend {
    fn new() -> Result<(PresenceAvailability, Self), String> {
        use tray_icon::menu::{Menu, MenuItem};
        let show = MenuItem::with_id("filebeam-show", "Show Filebeam", true, None);
        let quit = MenuItem::with_id("filebeam-quit", "Quit Filebeam", true, None);
        let menu = Menu::with_items(&[&show, &quit]).map_err(|error| error.to_string())?;
        let image = image::load_from_memory(include_bytes!("../../../backend/public/icon-192.png"))
            .map_err(|error| format!("decode Filebeam tray icon: {error}"))?
            .to_rgba8();
        let (width, height) = image.dimensions();
        let icon = tray_icon::Icon::from_rgba(image.into_raw(), width, height)
            .map_err(|error| error.to_string())?;
        let tray = tray_icon::TrayIconBuilder::new()
            .with_tooltip("Filebeam")
            .with_menu(Box::new(menu))
            .with_icon(icon)
            .build()
            .map_err(|error| error.to_string())?;
        Ok((
            PresenceAvailability {
                available: true,
                reason: None,
            },
            Self::Active { tray, show, quit },
        ))
    }

    fn try_event(&self) -> Option<PresenceEvent> {
        let Self::Active { tray, show, quit } = self;
        let _ = tray;
        if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == *show.id() {
                return Some(PresenceEvent::Show);
            }
            if event.id == *quit.id() {
                return Some(PresenceEvent::Quit);
            }
        }
        if matches!(
            tray_icon::TrayIconEvent::receiver().try_recv(),
            Ok(tray_icon::TrayIconEvent::Click { .. })
                | Ok(tray_icon::TrayIconEvent::DoubleClick { .. })
        ) {
            return Some(PresenceEvent::Show);
        }
        None
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
enum Backend {
    Unavailable,
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
impl Backend {
    fn new() -> Result<(PresenceAvailability, Self), String> {
        Ok((
            PresenceAvailability {
                available: false,
                reason: Some("native presence is unsupported on this platform".into()),
            },
            Self::Unavailable,
        ))
    }
    fn try_event(&self) -> Option<PresenceEvent> {
        None
    }
}
