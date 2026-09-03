//! Modal implementation for the which-key display.

use gpui::{
    App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, KeybindingKeystroke,
    ScrollHandle, Subscription, WeakEntity, Window,
};
use settings::{Settings, WhichKeyLayout, WhichKeyPosition};
use std::rc::Rc;
use theme_settings::ThemeSettings;
use ui::{DynamicSpacing, prelude::*};
use workspace::{ModalView, Workspace};

use crate::{
    bindings_for_which_key, map_pending_keystrokes,
    pending_bindings::{PendingBindingRow, PendingBindings, prepare_pending_bindings},
    which_key_settings::WhichKeySettings,
};

pub struct WhichKeyModal {
    _workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    bindings: Rc<[PendingBindingRow]>,
    pending_keys: Rc<[KeybindingKeystroke]>,
    _pending_input_subscription: Subscription,
    _focus_out_subscription: Subscription,
}

impl WhichKeyModal {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Keep focus where it currently is
        let focus_handle = window.focused(cx).unwrap_or(cx.focus_handle());

        let handle = cx.weak_entity();
        let mut this = Self {
            _workspace: workspace,
            focus_handle: focus_handle.clone(),
            scroll_handle: ScrollHandle::new(),
            bindings: Rc::from([]),
            pending_keys: Rc::from([]),
            _pending_input_subscription: cx.observe_pending_input(
                window,
                |this: &mut Self, window, cx| {
                    this.update_pending_keys(window, cx);
                },
            ),
            _focus_out_subscription: window.on_focus_out(&focus_handle, cx, move |_, _, cx| {
                handle.update(cx, |_, cx| cx.emit(DismissEvent)).ok();
            }),
        };
        this.update_pending_keys(window, cx);
        this
    }

    pub fn dismiss(&self, cx: &mut Context<Self>) {
        cx.emit(DismissEvent)
    }

    fn update_pending_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending_keys) = window.pending_input_keystrokes() else {
            cx.emit(DismissEvent);
            return;
        };
        self.bindings =
            prepare_pending_bindings(bindings_for_which_key(window, pending_keys), cx).into();
        let pending_keys = map_pending_keystrokes(pending_keys, cx.keyboard_mapper().as_ref());
        if self.pending_keys.as_ref() != pending_keys.as_slice() {
            self.scroll_handle.set_offset(Default::default());
        }
        self.pending_keys = pending_keys.into();
    }
}

impl Render for WhichKeyModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = *WhichKeySettings::get_global(cx);
        let viewport_size = window.viewport_size();
        let window_margin = px(16.);
        let max_content_height = px(f32::from(viewport_size.height) * 0.4);
        let max_panel_width = match settings.layout {
            WhichKeyLayout::List => px((f32::from(viewport_size.width) * 0.5).min(480.0)),
            WhichKeyLayout::Columns => viewport_size.width - window_margin * 2.0,
        };

        // Push above status bar when visible
        let status_height = self
            ._workspace
            .upgrade()
            .and_then(|workspace| {
                workspace.read_with(cx, |workspace, cx| {
                    if workspace.status_bar_visible(cx) {
                        Some(
                            DynamicSpacing::Base04.px(cx) * 2.0
                                + ThemeSettings::get_global(cx).ui_font_size(cx),
                        )
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(px(0.));

        let panel = div()
            .id("which-key-buffer-panel-scroll")
            .occlude()
            .min_w(px(220.))
            .max_w(max_panel_width)
            .elevation_3(cx)
            .overflow_hidden()
            .child(PendingBindings::new(
                "which-key-content",
                self.pending_keys.clone(),
                self.bindings.clone(),
                self.scroll_handle.clone(),
                max_content_height,
                settings.layout,
            ));

        let (horizontal, vertical) = anchors(settings.position);
        let bottom_padding = match vertical {
            VerticalAnchor::Bottom => window_margin + status_height,
            VerticalAnchor::Top | VerticalAnchor::Center => window_margin,
        };

        // The wrapper has no hitbox, so mouse input outside the panel still reaches the editor.
        h_flex()
            .absolute()
            .inset_0()
            .size_full()
            .px(window_margin)
            .pt(window_margin)
            .pb(bottom_padding)
            .map(|wrapper| match horizontal {
                HorizontalAnchor::Left => wrapper.justify_start(),
                HorizontalAnchor::Center => wrapper.justify_center(),
                HorizontalAnchor::Right => wrapper.justify_end(),
            })
            .map(|wrapper| match vertical {
                VerticalAnchor::Top => wrapper.items_start(),
                VerticalAnchor::Center => wrapper.items_center(),
                VerticalAnchor::Bottom => wrapper.items_end(),
            })
            .child(panel)
    }
}

impl EventEmitter<DismissEvent> for WhichKeyModal {}

impl Focusable for WhichKeyModal {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModalView for WhichKeyModal {
    fn render_bare(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HorizontalAnchor {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerticalAnchor {
    Top,
    Center,
    Bottom,
}

fn anchors(position: WhichKeyPosition) -> (HorizontalAnchor, VerticalAnchor) {
    match position {
        WhichKeyPosition::TopLeft => (HorizontalAnchor::Left, VerticalAnchor::Top),
        WhichKeyPosition::TopCenter => (HorizontalAnchor::Center, VerticalAnchor::Top),
        WhichKeyPosition::TopRight => (HorizontalAnchor::Right, VerticalAnchor::Top),
        WhichKeyPosition::CenterLeft => (HorizontalAnchor::Left, VerticalAnchor::Center),
        WhichKeyPosition::Center => (HorizontalAnchor::Center, VerticalAnchor::Center),
        WhichKeyPosition::CenterRight => (HorizontalAnchor::Right, VerticalAnchor::Center),
        WhichKeyPosition::BottomLeft => (HorizontalAnchor::Left, VerticalAnchor::Bottom),
        WhichKeyPosition::BottomCenter => (HorizontalAnchor::Center, VerticalAnchor::Bottom),
        WhichKeyPosition::BottomRight => (HorizontalAnchor::Right, VerticalAnchor::Bottom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anchors_map_every_position() {
        assert_eq!(
            anchors(WhichKeyPosition::Center),
            (HorizontalAnchor::Center, VerticalAnchor::Center)
        );
        assert_eq!(
            anchors(WhichKeyPosition::TopCenter),
            (HorizontalAnchor::Center, VerticalAnchor::Top)
        );
        assert_eq!(
            anchors(WhichKeyPosition::BottomRight),
            (HorizontalAnchor::Right, VerticalAnchor::Bottom)
        );
    }
}
