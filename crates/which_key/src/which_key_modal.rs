//! Modal implementation for the which-key display.

use gpui::prelude::FluentBuilder;
use gpui::{
    App, Context, DismissEvent, EventEmitter, FocusHandle, Focusable, FontWeight, Hsla,
    KeybindingKeystroke, Pixels, ScrollHandle, Subscription, TextRun, WeakEntity, Window,
};
use settings::{Settings, WhichKeyLayout, WhichKeyPosition};
use std::collections::HashMap;
use theme_settings::ThemeSettings;
use ui::{
    Divider, DividerColor, DynamicSpacing, LabelSize, WithScrollbar, prelude::*,
    text_for_keybinding_keystrokes,
};
use workspace::{ModalView, Workspace};

use crate::{bindings_for_which_key, map_pending_keystrokes, which_key_settings::WhichKeySettings};

pub struct WhichKeyModal {
    _workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    bindings: Vec<(SharedString, SharedString)>,
    pending_keys: SharedString,
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
            bindings: Vec::new(),
            pending_keys: SharedString::new_static(""),
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
        let mut binding_data = bindings_for_which_key(window, pending_keys)
            .into_iter()
            .map(|binding| (binding.remaining_keystrokes, binding.action_name))
            .collect();

        binding_data = group_bindings(binding_data);

        // Sort bindings from shortest to longest, with groups last
        // Using stable sort to preserve relative order of equal elements
        binding_data.sort_by(|(keystrokes_a, action_a), (keystrokes_b, action_b)| {
            // Groups (actions starting with "+") should go last
            let is_group_a = action_a.starts_with('+');
            let is_group_b = action_b.starts_with('+');

            // First, separate groups from non-groups
            let group_cmp = is_group_a.cmp(&is_group_b);
            if group_cmp != std::cmp::Ordering::Equal {
                return group_cmp;
            }

            // Then sort by keystroke count
            let keystroke_cmp = keystrokes_a.len().cmp(&keystrokes_b.len());
            if keystroke_cmp != std::cmp::Ordering::Equal {
                return keystroke_cmp;
            }

            // Finally sort by text length, then lexicographically for full stability
            let text_a = text_for_keybinding_keystrokes(keystrokes_a, cx);
            let text_b = text_for_keybinding_keystrokes(keystrokes_b, cx);
            let text_len_cmp = text_a.len().cmp(&text_b.len());
            if text_len_cmp != std::cmp::Ordering::Equal {
                return text_len_cmp;
            }
            text_a.cmp(&text_b)
        });
        binding_data.dedup();
        let pending_keys = map_pending_keystrokes(pending_keys, cx.keyboard_mapper().as_ref());
        self.pending_keys = text_for_keybinding_keystrokes(&pending_keys, cx).into();
        self.bindings = binding_data
            .into_iter()
            .map(|(keystrokes, action)| {
                (
                    text_for_keybinding_keystrokes(&keystrokes, cx).into(),
                    action,
                )
            })
            .collect();
    }
}

impl Render for WhichKeyModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = *WhichKeySettings::get_global(cx);
        let has_rows = !self.bindings.is_empty();
        let viewport_size = window.viewport_size();
        let window_margin = px(16.);
        let panel_padding = px(12.);
        let max_content_height = px(f32::from(viewport_size.height) * 0.4);

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

        // Title section
        let title_section = {
            let mut column = v_flex().gap(px(0.)).child(
                div()
                    .child(
                        Label::new(self.pending_keys.clone())
                            .size(LabelSize::Default)
                            .weight(FontWeight::MEDIUM)
                            .color(Color::Accent),
                    )
                    .mb(px(2.)),
            );

            if has_rows {
                column = column.child(
                    div()
                        .child(Divider::horizontal().color(DividerColor::BorderFaded))
                        .mb(px(2.)),
                );
            }

            column
        };

        let (rows, max_panel_width) =
            match settings.layout {
                WhichKeyLayout::List => (
                    render_column(&self.bindings, None).into_any_element(),
                    px((f32::from(viewport_size.width) * 0.5).min(480.0)),
                ),
                WhichKeyLayout::Columns => {
                    let max_panel_width = viewport_size.width - window_margin * 2.0;
                    let available_width = max_panel_width - panel_padding * 2.0;
                    let column_gap = px(16.);
                    let key_action_gap = px(8.);
                    let key_width =
                        max_text_width(self.bindings.iter().map(|(keys, _)| keys), window, cx);
                    let action_width = px(f32::from(max_text_width(
                        self.bindings.iter().map(|(_, action)| action),
                        window,
                        cx,
                    ))
                    .min(320.0));
                    let column_width = key_width + key_action_gap + action_width;
                    let columns = column_count(
                        self.bindings.len(),
                        f32::from(available_width),
                        f32::from(column_width),
                        f32::from(column_gap),
                    );
                    let rows_per_column = rows_per_column(self.bindings.len(), columns);
                    (
                        h_flex()
                            .items_start()
                            .gap(column_gap)
                            .children(self.bindings.chunks(rows_per_column).map(|column| {
                                render_column(column, Some((key_width, action_width)))
                            }))
                            .into_any_element(),
                        max_panel_width,
                    )
                }
            };

        let content = div()
            .id("which-key-content")
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .h_full()
            .max_h(max_content_height)
            .child(rows);

        let panel = div()
            .id("which-key-buffer-panel-scroll")
            .occlude()
            .min_w(px(220.))
            .max_w(max_panel_width)
            .elevation_3(cx)
            .px(panel_padding)
            .child(v_flex().child(title_section).when(has_rows, |el| {
                el.child(
                    div()
                        .max_h(max_content_height)
                        .child(content)
                        .vertical_scrollbar_for(&self.scroll_handle, window, cx),
                )
            }));

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

/// Renders one keystroke/action column. Fixed widths keep every column of the grid aligned;
/// without them the actions column stretches to fill the panel and truncates when it overflows.
fn render_column(
    bindings: &[(SharedString, SharedString)],
    fixed_widths: Option<(Pixels, Pixels)>,
) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap(px(8.))
        .child(
            v_flex()
                .gap(px(4.))
                .flex_shrink_0()
                .when_some(fixed_widths, |column, (key_width, _)| column.w(key_width))
                .children(bindings.iter().map(|(keystrokes, _)| {
                    div()
                        .child(
                            Label::new(keystrokes.clone())
                                .size(LabelSize::Default)
                                .color(Color::Accent),
                        )
                        .text_align(gpui::TextAlign::Right)
                })),
        )
        .child(
            v_flex()
                .gap(px(4.))
                .map(|column| match fixed_widths {
                    Some((_, action_width)) => column.w(action_width).flex_none(),
                    None => column.flex_1().min_w_0(),
                })
                .children(bindings.iter().map(|(_, action_name)| {
                    let is_group = action_name.starts_with('+');
                    let label_color = if is_group {
                        Color::Success
                    } else {
                        Color::Default
                    };

                    div().child(
                        Label::new(action_name.clone())
                            .size(LabelSize::Default)
                            .color(label_color)
                            .single_line()
                            .truncate(),
                    )
                })),
        )
}

fn max_text_width<'a>(
    texts: impl Iterator<Item = &'a SharedString>,
    window: &Window,
    cx: &App,
) -> Pixels {
    let theme_settings = ThemeSettings::get_global(cx);
    let font_size = theme_settings.ui_font_size(cx);
    let text_system = window.text_system();
    texts
        .map(|text| {
            let run = TextRun {
                len: text.len(),
                font: theme_settings.ui_font.clone(),
                color: Hsla::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            text_system.layout_line(text, font_size, &[run], None).width
        })
        .fold(
            px(0.),
            |widest, width| if width > widest { width } else { widest },
        )
}

fn column_count(
    binding_count: usize,
    available_width: f32,
    column_width: f32,
    column_gap: f32,
) -> usize {
    if binding_count == 0 || column_width <= 0. {
        return 1;
    }
    // Gaps sit between columns only, so the last column gets the gap's width back.
    let fitting = ((available_width + column_gap) / (column_width + column_gap)).floor();
    let fitting = if fitting.is_finite() && fitting >= 1. {
        fitting as usize
    } else {
        1
    };
    fitting.min(binding_count)
}

fn rows_per_column(binding_count: usize, column_count: usize) -> usize {
    binding_count.div_ceil(column_count.max(1)).max(1)
}

fn group_bindings(
    binding_data: Vec<(Vec<KeybindingKeystroke>, SharedString)>,
) -> Vec<(Vec<KeybindingKeystroke>, SharedString)> {
    let mut groups: HashMap<
        Option<KeybindingKeystroke>,
        Vec<(Vec<KeybindingKeystroke>, SharedString)>,
    > = HashMap::new();

    // Group bindings by their first keystroke
    for (remaining_keystrokes, action_name) in binding_data {
        let first_key = remaining_keystrokes.first().cloned();
        groups
            .entry(first_key)
            .or_default()
            .push((remaining_keystrokes, action_name));
    }

    let mut result = Vec::new();

    for (first_key, mut group_bindings) in groups {
        // Remove duplicates within each group
        group_bindings.dedup_by_key(|(keystrokes, _)| keystrokes.clone());

        if let Some(first_key) = first_key
            && group_bindings.len() > 1
        {
            // This is a group - create a single entry with just the first keystroke
            let first_keystroke = vec![first_key];
            let count = group_bindings.len();
            result.push((first_keystroke, format!("+{} keybinds", count).into()));
        } else {
            // Not a group or empty keystrokes - add all bindings as-is
            result.append(&mut group_bindings);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    use gpui::Modifiers;
    use gpui::{
        Action as _, Entity, FocusHandle, InvalidKeystrokeError, KeyBinding, Keystroke,
        TestAppContext, VisualTestContext, actions,
    };

    use super::*;

    actions!(
        which_key_modal_test,
        [FirstBinding, SecondBinding, ThirdBinding]
    );

    struct TestView {
        focus_handle: FocusHandle,
    }

    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .key_context("WhichKeyModalTest")
                .track_focus(&self.focus_handle)
                .on_action(|_: &FirstBinding, _, _| {})
                .on_action(|_: &SecondBinding, _, _| {})
                .on_action(|_: &ThirdBinding, _, _| {})
        }
    }

    fn setup_modal_test<'a>(
        cx: &'a mut TestAppContext,
        bindings: impl IntoIterator<Item = KeyBinding>,
        pending_keystrokes: &str,
    ) -> (Entity<WhichKeyModal>, &'a mut VisualTestContext) {
        cx.update(|cx| cx.bind_keys(bindings));
        let (test_view, cx) = cx.add_window_view(|_, cx| TestView {
            focus_handle: cx.focus_handle(),
        });
        let focus_handle = test_view.read_with(cx, |test_view, _| test_view.focus_handle.clone());
        cx.update(|window, cx| {
            window.focus(&focus_handle, cx);
            window.activate_window();
        });
        cx.simulate_keystrokes(pending_keystrokes);
        cx.run_until_parked();
        cx.update(|window, _| assert!(window.has_pending_keystrokes()));

        let modal = cx.update(|window, cx| {
            cx.new(|cx| WhichKeyModal::new(WeakEntity::new_invalid(), window, cx))
        });
        (modal, cx)
    }

    #[test]
    fn test_group_bindings_preserves_keybinding_keystrokes() -> Result<(), InvalidKeystrokeError> {
        #[cfg(target_os = "windows")]
        let keystroke = KeybindingKeystroke::new(
            Keystroke::parse("ctrl-$")?,
            Modifiers::control_shift(),
            "4".to_owned(),
        );
        #[cfg(not(target_os = "windows"))]
        let keystroke = KeybindingKeystroke::from_keystroke(Keystroke::parse("ctrl-x")?);
        let binding_data = vec![(vec![keystroke.clone()], SharedString::from("test action"))];

        let grouped_bindings = group_bindings(binding_data);

        assert_eq!(
            grouped_bindings,
            vec![(vec![keystroke], SharedString::from("test action"))]
        );
        Ok(())
    }

    #[gpui::test]
    fn test_which_key_modal_groups_and_orders_pending_bindings(cx: &mut TestAppContext) {
        let (modal, cx) = setup_modal_test(
            cx,
            [
                KeyBinding::new("ctrl-b h", FirstBinding, Some("WhichKeyModalTest")),
                KeyBinding::new("ctrl-b h j", SecondBinding, Some("WhichKeyModalTest")),
                KeyBinding::new("ctrl-b k", ThirdBinding, Some("WhichKeyModalTest")),
            ],
            "ctrl-b",
        );

        let h = KeybindingKeystroke::from_keystroke(
            Keystroke::parse("h").expect("valid test keystroke"),
        );
        let k = KeybindingKeystroke::from_keystroke(
            Keystroke::parse("k").expect("valid test keystroke"),
        );
        let h_text = cx.update(|_, cx| text_for_keybinding_keystrokes(&[h], cx));
        let k_text = cx.update(|_, cx| text_for_keybinding_keystrokes(&[k], cx));
        let expected_bindings = vec![
            (
                k_text.into(),
                command_palette::humanize_action_name(ThirdBinding.name()).into(),
            ),
            (h_text.into(), SharedString::from("+2 keybinds")),
        ];

        assert_eq!(
            modal.read_with(cx, |modal, _| modal.bindings.clone()),
            expected_bindings
        );
    }

    #[test]
    fn test_column_count_fits_available_width() {
        assert_eq!(column_count(40, 1000., 200., 16.), 4);
        assert_eq!(column_count(2, 1000., 200., 16.), 2);
        assert_eq!(column_count(40, 100., 200., 16.), 1);
        assert_eq!(column_count(0, 1000., 200., 16.), 1);
        assert_eq!(column_count(40, 1000., 0., 16.), 1);
    }

    #[test]
    fn test_rows_per_column_fills_top_to_bottom() {
        assert_eq!(rows_per_column(10, 4), 3);
        assert_eq!(rows_per_column(8, 4), 2);
        assert_eq!(rows_per_column(0, 4), 1);

        let bindings: Vec<usize> = (0..10).collect();
        let columns: Vec<&[usize]> = bindings.chunks(rows_per_column(10, 4)).collect();
        assert_eq!(columns, vec![&[0, 1, 2][..], &[3, 4, 5], &[6, 7, 8], &[9]]);
    }

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
