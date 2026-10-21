//! Which palette is up: Omarchy's live theme where the desktop has one, else ours for the system's light or dark.
//! `RADIO4000_THEME=light|dark` fixes one, for screenshots.
use crate::style::{self, Palette};
use fastframe_theme::{Base, Catalog, DesktopThemes, Transition, Waker};

pub struct Theme {
    catalog: Catalog<Palette>,
    themes: Option<std::path::PathBuf>,
    waker: Waker,
    fixed: Option<Base>,
    #[cfg(target_os = "linux")]
    appearance: Option<appearance::Appearance>,
    applied: Option<Palette>,
    transition: Transition,
}

impl Theme {
    /// A demo follows nothing on the desktop, so its screenshots stay the same.
    pub fn new(ctx: &egui::Context, demo: bool) -> Self {
        let fixed = match std::env::var("RADIO4000_THEME").as_deref() {
            Ok("light") => Some(Base::Light),
            Ok("dark") => Some(Base::Dark),
            _ => None,
        };
        let repaint = ctx.clone();
        let waker = Waker::new(move || repaint.request_repaint());
        let mut catalog = Catalog::default();
        let themes = (!demo)
            .then(|| fastframe_dirs::Dirs::platform("", "", "radio4000"))
            .flatten()
            .map(|dirs| dirs.config.join("themes"));
        if themes.is_some() {
            catalog.enable_desktop_themes(DesktopThemes {
                slug: "radio4000",
                omarchy_template: include_str!("../contrib/omarchy/radio4000.json.tpl"),
                omarchy_previous_templates: &[],
                presets: false,
            });
        }
        let mut theme = Self {
            catalog,
            themes,
            waker,
            fixed,
            #[cfg(target_os = "linux")]
            appearance: (fixed.is_none() && !demo).then(|| {
                let ctx = ctx.clone();
                appearance::Appearance::spawn(move || ctx.request_repaint())
            }),
            applied: None,
            transition: Transition::default(),
        };
        theme.scan();
        theme
    }

    fn scan(&mut self) {
        if let Some(dir) = &self.themes {
            self.catalog.start(dir.clone(), None, &self.waker);
        }
    }

    /// Before drawing: take a changed palette, revealed from the middle as Omarchy does.
    pub fn update(&mut self, ctx: &egui::Context) {
        if self.catalog.needs_reload() {
            self.scan();
        }
        self.catalog.poll();
        let wanted = self.wanted();
        if self.applied == Some(wanted) {
            return;
        }
        if self.applied.is_some() {
            self.transition.begin(ctx);
            if self.transition.holding(ctx) {
                return;
            }
        }
        style::apply(ctx, wanted);
        self.applied = Some(wanted);
    }

    /// After drawing.
    pub fn paint(&mut self, ctx: &egui::Context) {
        self.transition.paint(ctx);
    }

    fn wanted(&self) -> Palette {
        if self.fixed.is_none()
            && let Some(theme) = self.catalog.system_theme()
        {
            return theme.palette;
        }
        // No answer, or no preference, stays dark: that's our look.
        #[cfg(target_os = "linux")]
        let dark = self
            .appearance
            .as_ref()
            .and_then(appearance::Appearance::dark)
            .unwrap_or(true);
        #[cfg(not(target_os = "linux"))]
        let dark = true;
        match self.fixed.unwrap_or(Base::from_dark(dark)) {
            Base::Dark => Palette::dark(),
            Base::Light => Palette::light(),
        }
    }
}

/// winit tells no light or dark on most Linux desktops; the desktop portal does, and says when it changes.
#[cfg(target_os = "linux")]
mod appearance {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};
    use zbus::zvariant::{OwnedValue, Value};

    const UNKNOWN: u8 = 0;
    const DARK: u8 = 1;
    const LIGHT: u8 = 2;
    const NAMESPACE: &str = "org.freedesktop.appearance";
    const KEY: &str = "color-scheme";

    pub struct Appearance(Arc<AtomicU8>);

    impl Appearance {
        pub fn spawn(wake: impl Fn() + Send + 'static) -> Self {
            let scheme = Arc::new(AtomicU8::new(UNKNOWN));
            let shared = scheme.clone();
            std::thread::spawn(move || {
                let _ = watch(&shared, &wake);
            });
            Self(scheme)
        }

        /// Whether the desktop prefers dark; `None` until it says, or when it has no preference.
        pub fn dark(&self) -> Option<bool> {
            match self.0.load(Ordering::Relaxed) {
                DARK => Some(true),
                LIGHT => Some(false),
                _ => None,
            }
        }
    }

    fn watch(scheme: &AtomicU8, wake: &dyn Fn()) -> zbus::Result<()> {
        let connection = zbus::blocking::Connection::session()?;
        let proxy = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )?;
        // Subscribe before reading, so a change in between is not missed.
        let changes = proxy.receive_signal("SettingChanged")?;
        let current: zbus::Result<OwnedValue> = proxy
            .call("ReadOne", &(NAMESPACE, KEY))
            .or_else(|_| proxy.call("Read", &(NAMESPACE, KEY)));
        if let Some(value) = current.ok().as_deref().and_then(number) {
            publish(scheme, value, wake);
        }
        for message in changes {
            if let Ok((namespace, key, value)) =
                message.body().deserialize::<(String, String, OwnedValue)>()
                && namespace == NAMESPACE
                && key == KEY
                && let Some(value) = number(&value)
            {
                publish(scheme, value, wake);
            }
        }
        Ok(())
    }

    /// 1 prefers dark, 2 light, 0 says nothing.
    fn publish(scheme: &AtomicU8, value: u32, wake: &dyn Fn()) {
        let next = match value {
            1 => DARK,
            2 => LIGHT,
            _ => UNKNOWN,
        };
        if scheme.swap(next, Ordering::Relaxed) != next {
            wake();
        }
    }

    /// The number, from the one or two variants it arrives in.
    fn number(value: &Value<'_>) -> Option<u32> {
        match value {
            Value::U32(value) => Some(*value),
            Value::Value(inner) => number(inner),
            _ => None,
        }
    }
}
