//! Overflow for a run of toolbar controls: controls that do not fit are
//! hidden from the end, a `›` button takes the space they leave, and the
//! button opens a panel holding the whole run.
use std::{cell::Cell, rc::Rc};

use gpui::{
    Anchor, AnyElement, App, AvailableSpace, Bounds, Context, DispatchPhase, Element, ElementId,
    Entity, FocusHandle, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId,
    InteractiveElement as _, IntoElement, KeyBinding, LayoutId, MouseMoveEvent, ParentElement as _,
    Pixels, Point, Rems, RenderOnce, SharedString, Style, Subscription, Window, deferred, div,
    point, prelude::FluentBuilder as _, px, size,
};
use gpui_base::{POPUP_PRIORITY, Positioner};
use smallvec::SmallVec;

use super::{Toolbar, ToolbarItem, toolbar_gap, toolbar_padding};
use crate::{
    IconName, Sizable, Size, ThemeStyled as _,
    actions::Cancel,
    button::{Button, ButtonVariants as _},
};

const PANEL_CONTEXT: &str = "ToolbarOverflowPanel";

/// Escape is a binding in the panel's own context, so it wins over an
/// application binding for Escape on an enclosing view.
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", Cancel, Some(PANEL_CONTEXT))]);
}

/// Widths closer than this compare as equal, so a layout rounded to device
/// pixels does not hide a control that exactly fits.
const FIT_TOLERANCE: Pixels = px(0.01);

/// How many leading controls stay in the row.
///
/// Controls are placed from the start with `gap` between them. When they all
/// fit in `available` they all stay and no `›` is needed: the last control
/// never makes room for the button. Otherwise a control stays only when it and
/// the `›` button (`chevron`, after another `gap`) both fit, and the first
/// control that does not, with everything after it, is hidden. This is
/// IntelliJ's auto-layout strategy for an overflowing toolbar.
pub(crate) fn visible_count(
    widths: &[Pixels],
    gap: Pixels,
    available: Pixels,
    chevron: Pixels,
) -> usize {
    let total = widths
        .iter()
        .copied()
        .fold(px(0.), |sum, width| sum + width)
        + gap * widths.len().saturating_sub(1) as f32;
    if total <= available + FIT_TOLERANCE {
        return widths.len();
    }
    let mut x = px(0.);
    for (index, width) in widths.iter().enumerate() {
        if x + *width + gap + chevron > available + FIT_TOLERANCE {
            return index;
        }
        x += *width + gap;
    }
    widths.len()
}

/// How the overflow panel was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarOverflowOpener {
    /// The pointer rested on, or clicked, the `›` button. Focus stays where
    /// it was, and the panel closes when the pointer leaves it.
    Pointer,
    /// Enter or Space on the focused `›` button. Focus moves into the panel,
    /// and Escape returns it to the button.
    Keyboard,
}

/// The retained state of a [`ToolbarOverflowRow`]: whether its panel is open,
/// and what the row measured in its last frame.
///
/// Create it once with `cx.new(|cx| ToolbarOverflow::new(window, cx))` and
/// keep the entity in the view that renders the row. The owner reads
/// [`ToolbarOverflow::is_open`] to decide whether to build the panel's
/// controls, and calls [`ToolbarOverflow::close`] when a command that the
/// panel does not hold runs.
pub struct ToolbarOverflow {
    opener: Option<ToolbarOverflowOpener>,
    /// The row's width when the panel opened; a different width closes it.
    opened_width: Option<Pixels>,
    row_bounds: Option<Bounds<Pixels>>,
    first_hidden: Option<usize>,
    chevron_bounds: Option<Bounds<Pixels>>,
    panel_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    last_pointer: Option<Point<Pixels>>,
    chevron_focus: FocusHandle,
    panel_focus: FocusHandle,
    focus_panel_pending: bool,
    _subscriptions: [Subscription; 2],
}

impl ToolbarOverflow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let panel_focus = cx.focus_handle().tab_stop(false);
        let subscriptions = [
            // Moving or resizing the window closes the panel.
            cx.observe_window_bounds(window, |this, _, cx| this.close(cx)),
            // Focus leaving a keyboard-opened panel closes it.
            cx.on_focus_out(&panel_focus, window, |this, _, _, cx| {
                if this.opener == Some(ToolbarOverflowOpener::Keyboard) {
                    this.close(cx);
                }
            }),
        ];
        Self {
            opener: None,
            opened_width: None,
            row_bounds: None,
            first_hidden: None,
            chevron_bounds: None,
            panel_bounds: Rc::default(),
            last_pointer: None,
            chevron_focus: cx.focus_handle().tab_stop(true),
            panel_focus,
            focus_panel_pending: false,
            _subscriptions: subscriptions,
        }
    }

    /// Whether the panel is open.
    pub fn is_open(&self) -> bool {
        self.opener.is_some()
    }

    /// How the open panel was opened, or `None` when it is closed.
    pub fn opener(&self) -> Option<ToolbarOverflowOpener> {
        self.opener
    }

    /// The index of the first hidden control in the last frame, or `None`
    /// when every control fit.
    pub fn first_hidden(&self) -> Option<usize> {
        self.first_hidden
    }

    /// Open the panel. Opening an open panel with the keyboard moves focus
    /// into it; otherwise an open panel stays as it is.
    pub fn open(&mut self, opener: ToolbarOverflowOpener, cx: &mut Context<Self>) {
        if self.opener == Some(opener) {
            return;
        }
        if self.opener.is_some() && opener == ToolbarOverflowOpener::Pointer {
            return;
        }
        self.opener = Some(opener);
        self.opened_width = self.row_bounds.map(|bounds| bounds.size.width);
        self.focus_panel_pending = opener == ToolbarOverflowOpener::Keyboard;
        cx.notify();
    }

    /// Close the panel.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.opener.take().is_none() {
            return;
        }
        self.opened_width = None;
        self.focus_panel_pending = false;
        self.panel_bounds.set(None);
        cx.notify();
    }

    /// The pointer moved to `position`; `over_chevron` says whether `›` is
    /// the topmost thing under it.
    fn pointer_moved(
        &mut self,
        position: Point<Pixels>,
        over_chevron: bool,
        cx: &mut Context<Self>,
    ) {
        // GPUI repeats the last pointer position after a frame changes what
        // lies under it. Only a real movement opens the panel, as IntelliJ
        // opens it from mouse motion, so a panel closed while the pointer
        // rests on `›` stays closed until the pointer moves.
        let moved = self.last_pointer.replace(position) != Some(position);
        match self.opener {
            None => {
                if moved && over_chevron {
                    self.open(ToolbarOverflowOpener::Pointer, cx);
                }
            }
            Some(ToolbarOverflowOpener::Pointer) => {
                // Geometry, not hover state: the panel covers `›`, so `›`
                // stops being hovered as soon as the panel opens.
                let inside =
                    |bounds: Option<Bounds<Pixels>>| bounds.is_some_and(|b| b.contains(&position));
                if !inside(self.chevron_bounds) && !inside(self.panel_bounds.get()) {
                    self.close(cx);
                }
            }
            Some(ToolbarOverflowOpener::Keyboard) => {}
        }
    }
}

/// A run of toolbar controls that hides the controls that do not fit.
///
/// Put it in a [`Toolbar`] with [`Toolbar::content`]; it takes the space the
/// toolbar leaves it, so give the toolbar a definite width and keep the
/// controls that must stay visible outside the row. Controls are kept from
/// the start while they fit, by the rule of [`visible_count`]; when any is
/// hidden, a `›` button fills the right end of the row. The row is never
/// narrower than that button.
///
/// The pointer resting on `›`, or a click on it, opens a panel over the
/// toolbar holding the controls given to [`ToolbarOverflowRow::panel`]
/// (normally the whole run, built a second time). Opening with the pointer
/// leaves focus where it is; Enter or Space moves focus into the panel.
/// The panel closes when the pointer leaves both it and `›`, on a press
/// outside it, when the window moves or resizes, when the row's width
/// changes, on Escape after a keyboard open, and on
/// [`ToolbarOverflow::close`].
///
/// Build the panel only while [`ToolbarOverflow::is_open`], so a closed row
/// does not build its controls twice. The panel's toolbar has its own id;
/// controls inside it keep their own ids, scoped under it.
#[derive(IntoElement)]
pub struct ToolbarOverflowRow {
    id: ElementId,
    state: Entity<ToolbarOverflow>,
    size: Size,
    more_label: SharedString,
    items: SmallVec<[ToolbarItem; 8]>,
    panel: Option<Toolbar>,
}

impl ToolbarOverflowRow {
    pub fn new(id: impl Into<ElementId>, state: &Entity<ToolbarOverflow>) -> Self {
        Self {
            id: id.into(),
            state: state.clone(),
            size: Size::Small,
            more_label: "More".into(),
            items: SmallVec::new(),
            panel: None,
        }
    }

    /// The accessible name of the `›` button. Defaults to "More".
    pub fn more_label(mut self, label: impl Into<SharedString>) -> Self {
        self.more_label = label.into();
        self
    }

    /// Append a control that inherits the row's size.
    pub fn child(mut self, child: impl Sizable + IntoElement + 'static) -> Self {
        self.items.push(ToolbarItem::sized(child));
        self
    }

    /// Append controls that inherit the row's size.
    pub fn children<T>(mut self, children: impl IntoIterator<Item = T>) -> Self
    where
        T: Sizable + IntoElement + 'static,
    {
        self.items
            .extend(children.into_iter().map(ToolbarItem::sized));
        self
    }

    /// Append non-sized content, such as a separator. It is hidden or kept as
    /// one unit, like a control.
    pub fn content(mut self, content: impl IntoElement) -> Self {
        self.items.push(ToolbarItem::content(content));
        self
    }

    /// Append non-sized content.
    pub fn contents(mut self, contents: impl IntoIterator<Item = AnyElement>) -> Self {
        self.items
            .extend(contents.into_iter().map(ToolbarItem::Content));
        self
    }

    /// The toolbar shown in the open panel. It takes the row's size.
    pub fn panel(mut self, panel: Toolbar) -> Self {
        self.panel = Some(panel);
        self
    }
}

impl Sizable for ToolbarOverflowRow {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = match size.into() {
            Size::Large => Size::Medium,
            size => size,
        };
        self
    }
}

impl RenderOnce for ToolbarOverflowRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = self.size;
        let state = self.state.clone();
        let (opener, row_bounds, chevron_focus, panel_focus, panel_bounds) = {
            let overflow = state.read(cx);
            (
                overflow.opener,
                overflow.row_bounds,
                overflow.chevron_focus.clone(),
                overflow.panel_focus.clone(),
                overflow.panel_bounds.clone(),
            )
        };

        let chevron = Button::new("toolbar-overflow-more")
            .ghost()
            .compact()
            .with_size(size)
            .icon(IconName::ChevronRight)
            .accessibility_label(self.more_label)
            .track_focus(&chevron_focus)
            .on_click({
                let state = state.clone();
                move |event, _, cx| {
                    let opener = if event.is_keyboard() {
                        ToolbarOverflowOpener::Keyboard
                    } else {
                        ToolbarOverflowOpener::Pointer
                    };
                    state.update(cx, |state, cx| state.open(opener, cx));
                }
            })
            .into_any_element();

        let panel = match (opener, row_bounds, self.panel) {
            (Some(opener), Some(row_bounds), Some(panel)) => {
                // Lay the panel's controls over the row's: its top-left corner
                // sits one toolbar padding up and left of the row.
                let inset = toolbar_padding(size).to_pixels(window.rem_size());
                let origin = row_bounds.origin - point(inset, inset);
                let pointer = opener == ToolbarOverflowOpener::Pointer;
                let surface = div()
                    .id("toolbar-overflow-panel")
                    .track_focus(&panel_focus)
                    .popover_style(cx)
                    // Opened by the pointer, the panel must not take focus:
                    // GPUI focuses a pressed element only while the default is
                    // not prevented, and clicks do not depend on it.
                    .when(pointer, |this| {
                        this.capture_any_mouse_down(|_, window, _| window.prevent_default())
                    })
                    .on_mouse_down_out({
                        let state = state.clone();
                        move |_, _, cx| state.update(cx, |state, cx| state.close(cx))
                    })
                    // Only a keyboard open puts focus in the panel, so only then
                    // does its context, and Escape, apply.
                    .key_context(PANEL_CONTEXT)
                    .on_action({
                        let state = state.clone();
                        let chevron_focus = chevron_focus.clone();
                        move |_: &Cancel, window, cx| {
                            state.update(cx, |state, cx| state.close(cx));
                            window.focus(&chevron_focus, cx);
                        }
                    })
                    .child(panel.with_size(size));
                Some(
                    deferred(
                        Positioner::corner(Anchor::TopLeft, origin)
                            .occlude()
                            .on_position(move |position| panel_bounds.set(Some(position.bounds)))
                            .child(surface),
                    )
                    .with_priority(POPUP_PRIORITY)
                    .into_any_element(),
                )
            }
            _ => None,
        };

        OverflowRowElement {
            id: self.id,
            state,
            gap: toolbar_gap(size),
            items: self
                .items
                .into_iter()
                .map(|item| item.into_element(size))
                .collect(),
            chevron,
            panel,
        }
    }
}

/// The element behind [`ToolbarOverflowRow`]. Only `›` (and the deferred
/// panel) take part in the parent's layout, which makes `›` the row's minimum
/// width. The controls are measured in `prepaint`, as their own layout roots,
/// so the row decides what to hide in the same frame its width changes.
struct OverflowRowElement {
    id: ElementId,
    state: Entity<ToolbarOverflow>,
    gap: Rems,
    items: Vec<AnyElement>,
    chevron: AnyElement,
    panel: Option<AnyElement>,
}

struct RowLayout {
    chevron: LayoutId,
}

struct RowPrepaint {
    /// Present while `›` is shown.
    chevron_hitbox: Option<Hitbox>,
    panel: bool,
    open: bool,
}

impl IntoElement for OverflowRowElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for OverflowRowElement {
    type RequestLayoutState = RowLayout;
    type PrepaintState = RowPrepaint;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let chevron = self.chevron.request_layout(window, cx);
        let mut children = vec![chevron];
        if let Some(panel) = self.panel.as_mut() {
            children.push(panel.request_layout(window, cx));
        }
        let style = Style {
            display: gpui::Display::Flex,
            flex_grow: 1.,
            flex_shrink: 1.,
            justify_content: Some(gpui::JustifyContent::FlexEnd),
            align_items: Some(gpui::AlignItems::Center),
            ..Style::default()
        };
        (
            window.request_layout(style, children, cx),
            RowLayout { chevron },
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let gap = self.gap.to_pixels(window.rem_size());
        let height = bounds.size.height;
        let available = size(AvailableSpace::MaxContent, AvailableSpace::Definite(height));
        let sizes: Vec<_> = self
            .items
            .iter_mut()
            .map(|item| item.layout_as_root(available, window, cx))
            .collect();
        let widths: Vec<_> = sizes.iter().map(|size| size.width).collect();
        let chevron_bounds = window.layout_bounds(layout.chevron);
        let visible = visible_count(&widths, gap, bounds.size.width, chevron_bounds.size.width);

        self.items.truncate(visible);
        let mut x = bounds.origin.x;
        for (item, size) in self.items.iter_mut().zip(&sizes) {
            let y = bounds.origin.y + (height - size.height) / 2.;
            item.prepaint_at(point(x, y), window, cx);
            x += size.width + gap;
        }

        let hidden = visible < sizes.len();
        let chevron_hitbox = hidden.then(|| {
            self.chevron.prepaint(window, cx);
            window.insert_hitbox(chevron_bounds, HitboxBehavior::Normal)
        });

        let (opener, width_changed) = self.state.update(cx, |state, cx| {
            state.row_bounds = Some(bounds);
            state.first_hidden = hidden.then_some(visible);
            state.chevron_bounds = hidden.then_some(chevron_bounds);
            let width_changed = state
                .opened_width
                .is_some_and(|width| width != bounds.size.width);
            if width_changed {
                state.close(cx);
            }
            (state.opener, width_changed)
        });

        let panel = !width_changed && self.panel.is_some();
        if panel {
            if let Some(panel) = self.panel.as_mut() {
                panel.prepaint(window, cx);
            }
            let focus_panel = self.state.update(cx, |state, _| {
                std::mem::take(&mut state.focus_panel_pending).then(|| state.panel_focus.clone())
            });
            if let Some(panel_focus) = focus_panel {
                // The panel's controls join the tab order when this frame is
                // drawn, so step into them once it is.
                window.defer(cx, move |window, cx| {
                    focus_first_within(&panel_focus, window, cx)
                });
            }
        }

        RowPrepaint {
            chevron_hitbox,
            panel,
            open: opener.is_some(),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for item in &mut self.items {
            item.paint(window, cx);
        }
        if prepaint.chevron_hitbox.is_some() {
            self.chevron.paint(window, cx);
        }
        if prepaint.panel
            && let Some(panel) = self.panel.as_mut()
        {
            panel.paint(window, cx);
        }

        // The pointer opens the panel from `›` and closes it on leaving.
        if prepaint.chevron_hitbox.is_some() || prepaint.open {
            let hitbox = prepaint.chevron_hitbox.clone();
            let state = self.state.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let over_chevron = hitbox
                    .as_ref()
                    .is_some_and(|hitbox| hitbox.is_hovered(window));
                state.update(cx, |state, cx| {
                    state.pointer_moved(event.position, over_chevron, cx)
                });
            });
        }
    }
}

/// Move focus to the first tab stop inside `container`, walking the window's
/// tab order forward from the current focus. Focus stays put when the
/// container has no reachable stop.
fn focus_first_within(container: &FocusHandle, window: &mut Window, cx: &mut App) {
    let start = window.focused(cx);
    let mut visited = Vec::new();
    loop {
        window.focus_next(cx);
        let Some(focused) = window.focused(cx) else {
            break;
        };
        if container.contains_focused(window, cx) {
            return;
        }
        if visited.contains(&focused) || start.as_ref() == Some(&focused) {
            break;
        }
        visited.push(focused);
    }
    if let Some(start) = start {
        window.focus(&start, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths(values: &[f32]) -> Vec<Pixels> {
        values.iter().copied().map(px).collect()
    }

    #[test]
    fn every_control_that_fits_stays() {
        assert_eq!(
            visible_count(&widths(&[20., 20., 1.]), px(4.), px(100.), px(24.)),
            3
        );
    }

    #[test]
    fn the_last_control_needs_no_room_for_the_chevron() {
        // 20 + 4 + 20 + 4 + 20 = 68: the last one fits alone, not with `›`.
        assert_eq!(
            visible_count(&widths(&[20., 20., 20.]), px(4.), px(68.), px(24.)),
            3
        );
        assert_eq!(
            visible_count(&widths(&[20., 20., 20.]), px(4.), px(67.), px(24.)),
            1
        );
    }

    #[test]
    fn a_control_stays_only_with_room_for_the_chevron() {
        // The first control needs 20 + 4 + 24 = 48; the second 20 + 4 + 20 + 4 + 24 = 72.
        let controls = widths(&[20., 20., 20., 20.]);
        assert_eq!(visible_count(&controls, px(4.), px(71.), px(24.)), 1);
        assert_eq!(visible_count(&controls, px(4.), px(72.), px(24.)), 2);
    }

    #[test]
    fn the_cut_can_fall_right_after_a_separator() {
        // up, down, separator, previous file, counter: the separator needs
        // 24 + 4 + 24 + 4 + 1 + 4 + 24 = 85 with `›`, the file button 113.
        let controls = widths(&[24., 24., 1., 24., 24.]);
        assert_eq!(visible_count(&controls, px(4.), px(86.), px(24.)), 3);
    }

    #[test]
    fn only_the_chevron_fits() {
        assert_eq!(
            visible_count(&widths(&[24., 24.]), px(4.), px(24.), px(24.)),
            0
        );
    }

    #[test]
    fn nothing_fits_in_no_width() {
        assert_eq!(visible_count(&widths(&[24.]), px(4.), px(0.), px(24.)), 0);
        assert_eq!(visible_count(&[], px(4.), px(0.), px(24.)), 0);
    }

    #[cfg(feature = "test-support")]
    mod behavior {
        use std::{cell::Cell, rc::Rc};

        use gpui::{
            AppContext as _, Context, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseButton,
            MouseDownEvent, Render, Styled as _, TestAppContext, VisualTestContext, div, point, px,
            size,
        };
        use gpui_base::test_support::{ElementSnapshot, find, find_all};

        use super::super::*;
        use crate::{h_flex, toolbar::Toolbar};

        const CONTROLS: [&str; 5] = ["one", "two", "three", "four", "five"];

        struct Harness {
            overflow: Entity<ToolbarOverflow>,
            width: Pixels,
            /// The toolbar sits at the window's top-right corner instead of
            /// clear of the edges.
            at_edge: bool,
            clicks: Rc<Cell<Vec<&'static str>>>,
            body: FocusHandle,
        }

        impl Harness {
            fn controls(&self) -> Vec<Button> {
                CONTROLS
                    .into_iter()
                    .map(|id| {
                        let clicks = self.clicks.clone();
                        Button::new(id)
                            .icon(IconName::Plus)
                            .accessibility_label(id)
                            .on_click(move |_, _, _| {
                                let mut seen = clicks.take();
                                seen.push(id);
                                clicks.set(seen);
                            })
                    })
                    .collect()
            }
        }

        impl Render for Harness {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let open = self.overflow.read(cx).is_open();
                let row = ToolbarOverflowRow::new("row", &self.overflow)
                    .children(self.controls())
                    .when(open, |row| {
                        row.panel(Toolbar::new("panel").children(self.controls()))
                    });
                div()
                    .size_full()
                    .child(
                        h_flex()
                            .w_full()
                            .map(|this| {
                                if self.at_edge {
                                    this.justify_end()
                                } else {
                                    this.pt(px(40.)).pl(px(40.))
                                }
                            })
                            .child(
                                div().w(self.width).child(
                                    Toolbar::new("bar")
                                        .w_full()
                                        .content(row)
                                        .child(Button::new("tail").label("Tail")),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .id("body")
                            .track_focus(&self.body)
                            .mt(px(100.))
                            .size(px(100.)),
                    )
            }
        }

        fn harness(
            cx: &mut TestAppContext,
            width: f32,
        ) -> (Entity<Harness>, &mut VisualTestContext) {
            harness_at(cx, width, false)
        }

        fn harness_at(
            cx: &mut TestAppContext,
            width: f32,
            at_edge: bool,
        ) -> (Entity<Harness>, &mut VisualTestContext) {
            cx.update(crate::init);
            let (view, cx) = cx.add_window_view(move |window, cx| {
                let body = cx.focus_handle().tab_stop(true);
                body.focus(window, cx);
                Harness {
                    overflow: cx.new(|cx| ToolbarOverflow::new(window, cx)),
                    width: px(width),
                    at_edge,
                    clicks: Rc::default(),
                    body,
                }
            });
            cx.run_until_parked();
            (view, cx)
        }

        fn overflow(view: &Entity<Harness>, cx: &mut VisualTestContext) -> Entity<ToolbarOverflow> {
            cx.update(|_, cx| view.read(cx).overflow.clone())
        }

        fn is_open(view: &Entity<Harness>, cx: &mut VisualTestContext) -> bool {
            overflow(view, cx).read_with(cx, |state, _| state.is_open())
        }

        /// The control painted in the toolbar row, not the panel.
        fn in_row(id: &str, cx: &mut VisualTestContext) -> Option<ElementSnapshot> {
            in_scope(id, false, cx)
        }

        fn in_panel(id: &str, cx: &mut VisualTestContext) -> Option<ElementSnapshot> {
            in_scope(id, true, cx)
        }

        fn in_scope(id: &str, panel: bool, cx: &mut VisualTestContext) -> Option<ElementSnapshot> {
            let id = ElementId::from(SharedString::from(id.to_string()));
            let panel_id = ElementId::from("toolbar-overflow-panel");
            cx.update(|window, _| {
                find_all(window, &[], &id)
                    .into_iter()
                    .find(|entry| entry.path().contains(&panel_id) == panel)
            })
        }

        fn chevron(cx: &mut VisualTestContext) -> Option<ElementSnapshot> {
            cx.update(|window, _| find(window, &[], &ElementId::from("toolbar-overflow-more")))
        }

        fn set_width(view: &Entity<Harness>, width: f32, cx: &mut VisualTestContext) {
            view.update(cx, |harness, cx| {
                harness.width = px(width);
                cx.notify();
            });
            cx.run_until_parked();
        }

        fn hover_chevron(view: &Entity<Harness>, cx: &mut VisualTestContext) {
            let chevron = chevron(cx).expect("› is painted");
            cx.simulate_mouse_move(chevron.bounds().center(), None, Modifiers::none());
            cx.run_until_parked();
            assert!(is_open(view, cx));
        }

        fn body_focused(view: &Entity<Harness>, cx: &mut VisualTestContext) -> bool {
            let body = cx.update(|_, cx| view.read(cx).body.clone());
            cx.update(|window, _| body.is_focused(window))
        }

        #[gpui::test]
        fn a_narrow_row_hides_controls_from_the_end_and_shows_the_chevron(cx: &mut TestAppContext) {
            let (view, cx) = harness(cx, 140.);

            let hidden = overflow(&view, cx)
                .read_with(cx, |state, _| state.first_hidden())
                .expect("some controls are hidden");
            assert!(hidden > 0);
            for (index, id) in CONTROLS.into_iter().enumerate() {
                assert_eq!(in_row(id, cx).is_some(), index < hidden, "{id}");
            }

            // `›` sits at the right end of the row, one gap before the tail.
            let chevron = chevron(cx).expect("› is painted").bounds();
            let tail = in_row("tail", cx).expect("tail is painted").bounds();
            assert_eq!(chevron.right() + px(4.), tail.left());
            let last = in_row(CONTROLS[hidden - 1], cx).unwrap().bounds();
            assert!(last.right() <= chevron.left());
        }

        #[gpui::test]
        fn widening_the_row_brings_controls_back_in_order(cx: &mut TestAppContext) {
            let (view, cx) = harness(cx, 60.);
            let mut shown = 0;
            for width in (60..400).step_by(4) {
                set_width(&view, width as f32, cx);
                let now = CONTROLS
                    .into_iter()
                    .take_while(|id| in_row(id, cx).is_some())
                    .count();
                assert!(now >= shown, "controls only come back as the row widens");
                assert!(CONTROLS[now..].iter().all(|id| in_row(id, cx).is_none()));
                assert_eq!(chevron(cx).is_some(), now < CONTROLS.len());
                shown = now;
            }
            assert_eq!(shown, CONTROLS.len());
        }

        #[gpui::test]
        fn the_pointer_opens_the_panel_without_moving_focus(cx: &mut TestAppContext) {
            let (view, cx) = harness(cx, 140.);
            hover_chevron(&view, cx);

            for id in CONTROLS {
                assert!(in_panel(id, cx).is_some(), "{id} is in the panel");
            }
            // The panel's controls lie over the row's.
            let row = in_row("one", cx).unwrap().bounds();
            let panel = in_panel("one", cx).unwrap().bounds();
            assert_eq!(row.origin, panel.origin);
            assert!(body_focused(&view, cx));
        }

        #[gpui::test]
        fn clicking_in_a_pointer_panel_runs_the_control_and_keeps_it_open(cx: &mut TestAppContext) {
            let (view, cx) = harness(cx, 140.);
            hover_chevron(&view, cx);

            let five = in_panel("five", cx).unwrap().bounds().center();
            cx.simulate_mouse_move(five, None, Modifiers::none());
            cx.simulate_click(five, Modifiers::none());
            cx.run_until_parked();

            assert_eq!(cx.update(|_, cx| view.read(cx).clicks.take()), vec!["five"]);
            assert!(is_open(&view, cx));
            assert!(body_focused(&view, cx));
        }

        #[gpui::test]
        fn the_pointer_leaving_closes_the_panel(cx: &mut TestAppContext) {
            let (view, cx) = harness(cx, 140.);
            hover_chevron(&view, cx);

            let five = in_panel("five", cx).unwrap().bounds().center();
            cx.simulate_mouse_move(five, None, Modifiers::none());
            assert!(is_open(&view, cx));

            cx.simulate_mouse_move(point(px(50.), px(150.)), None, Modifiers::none());
            assert!(!is_open(&view, cx));
        }

        #[gpui::test]
        fn a_press_outside_closes_the_panel(cx: &mut TestAppContext) {
            let (view, cx) = harness(cx, 140.);
            hover_chevron(&view, cx);
            // Keep the pointer inside, so only the press can close the panel.
            let five = in_panel("five", cx).unwrap().bounds().center();
            cx.simulate_mouse_move(five, None, Modifiers::none());

            cx.simulate_event(MouseDownEvent {
                position: point(px(50.), px(150.)),
                modifiers: Modifiers::none(),
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
            });
            assert!(!is_open(&view, cx));
        }

        #[gpui::test]
        fn a_width_change_closes_the_panel(cx: &mut TestAppContext) {
            let (view, cx) = harness(cx, 140.);
            hover_chevron(&view, cx);
            set_width(&view, 150., cx);
            assert!(!is_open(&view, cx));

            hover_chevron(&view, cx);
            cx.simulate_resize(size(px(900.), px(700.)));
            cx.run_until_parked();
            assert!(!is_open(&view, cx));
        }

        #[gpui::test]
        fn close_closes_the_panel(cx: &mut TestAppContext) {
            let (view, cx) = harness(cx, 140.);
            hover_chevron(&view, cx);
            overflow(&view, cx).update(cx, |state, cx| state.close(cx));
            cx.run_until_parked();
            assert!(in_panel("one", cx).is_none());
        }

        #[gpui::test]
        fn a_panel_closed_under_a_resting_pointer_reopens_only_when_it_moves(
            cx: &mut TestAppContext,
        ) {
            let (view, cx) = harness(cx, 140.);
            hover_chevron(&view, cx);
            let at = chevron(cx).unwrap().bounds().center();
            overflow(&view, cx).update(cx, |state, cx| state.close(cx));
            cx.run_until_parked();
            cx.simulate_mouse_move(at, None, Modifiers::none());
            assert!(!is_open(&view, cx));

            cx.simulate_mouse_move(at + point(px(1.), px(0.)), None, Modifiers::none());
            assert!(is_open(&view, cx));
        }

        #[gpui::test]
        fn the_panel_stays_inside_the_window(cx: &mut TestAppContext) {
            let (view, cx) = harness_at(cx, 60., true);
            hover_chevron(&view, cx);

            let viewport = cx.update(|window, _| window.viewport_size());
            let five = in_panel("five", cx).expect("five is in the panel").bounds();
            assert!(five.right() <= viewport.width);
            let panel_one = in_panel("one", cx).unwrap().bounds();
            let chevron = chevron(cx).unwrap().bounds();
            assert!(
                panel_one.left() < chevron.left(),
                "the panel moved left to fit"
            );
        }

        #[gpui::test]
        fn the_keyboard_opens_the_panel_with_focus_inside_and_escape_returns_it(
            cx: &mut TestAppContext,
        ) {
            let (view, cx) = harness(cx, 140.);
            let state = overflow(&view, cx);
            let chevron_focus = state.read_with(cx, |state, _| state.chevron_focus.clone());
            cx.update(|window, cx| window.focus(&chevron_focus, cx));
            cx.run_until_parked();

            let enter = Keystroke::parse("enter").unwrap();
            cx.simulate_event(KeyDownEvent {
                keystroke: enter.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            cx.simulate_event(KeyUpEvent { keystroke: enter });
            cx.run_until_parked();
            assert_eq!(
                state.read_with(cx, |state, _| state.opener()),
                Some(ToolbarOverflowOpener::Keyboard)
            );
            assert_eq!(in_panel("one", cx).unwrap().focused(), Some(true));

            cx.simulate_keystrokes("right right");
            assert_eq!(in_panel("three", cx).unwrap().focused(), Some(true));

            cx.simulate_keystrokes("escape");
            cx.run_until_parked();
            assert!(!state.read_with(cx, |state, _| state.is_open()));
            assert!(cx.update(|window, _| chevron_focus.is_focused(window)));
        }
    }
}
