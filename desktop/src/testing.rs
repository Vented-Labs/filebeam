//! Shared fixtures for view tests.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use filebeam_client_config::{Appearance, Theme};
use gpui::{
    AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled,
    TestAppContext, Window, WindowHandle, div, px, size,
};
use gpui_component::Root;

use crate::{
    client::DesktopClient,
    live::Live,
    model::{ClientCommand, DesktopSnapshot},
};

pub type Commands = std::sync::mpsc::Receiver<ClientCommand>;

/// Installs the component library and a dark theme without motion.
pub fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        crate::theme::apply(
            &Appearance {
                theme: Theme::Dark,
                reduced_motion: true,
            },
            cx,
        );
    });
}

/// Stands in for the shell: the view under test plus the overlay layers.
struct Host(AnyView);

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(self.0.clone())
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

/// Opens a view inside the component root, as the app hosts it, so dialogs and
/// notifications have somewhere to open. The view reads `snapshot` through `Live`.
pub fn rooted<V: Render + 'static>(
    cx: &mut TestAppContext,
    snapshot: DesktopSnapshot,
    build: impl FnOnce(Entity<Live>, &mut Window, &mut Context<V>) -> V + 'static,
) -> (WindowHandle<Root>, Entity<V>, Commands) {
    init(cx);
    let (client, commands) = DesktopClient::test_client(snapshot);
    let slot = Rc::new(RefCell::new(None));
    let built = slot.clone();
    let window = cx.open_window(size(px(1280.), px(820.)), move |window, cx| {
        let live = cx.new(|cx| Live::new(Arc::new(client), cx));
        let view = cx.new(|cx| build(live, window, cx));
        *built.borrow_mut() = Some(view.clone());
        let host = cx.new(|_| Host(view.into()));
        Root::new(host, window, cx)
    });
    let view = slot.borrow_mut().take().expect("the window built its view");
    (window, view, commands)
}

/// Runs `update` in the window without borrowing its root, which dialogs update.
pub fn in_window<R>(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
    update: impl FnOnce(&mut Window, &mut App) -> R,
) -> R {
    cx.update_window(window.into(), |_, window, cx| update(window, cx))
        .expect("the test window is open")
}
