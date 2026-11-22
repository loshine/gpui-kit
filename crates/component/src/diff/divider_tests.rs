use std::{cell::RefCell, rc::Rc};

use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Render, StatefulInteractiveElement as _, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px,
};

use super::{
    Diff, DiffGutterMarker, DiffLinePosition, DiffMode, DiffSide, DiffState, document::fixture,
};

fn position(side: DiffSide, line: usize) -> DiffLinePosition {
    DiffLinePosition::new("file.txt", side, line)
}

struct Host {
    state: Entity<DiffState>,
    markers: bool,
    renderer: bool,
    clicked: Rc<RefCell<Vec<DiffLinePosition>>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicked = self.clicked.clone();
        let mut diff = Diff::new(&self.state).size_full();
        if self.markers {
            diff = diff.divider_markers([
                // A row that only deletes and a row that only adds.
                DiffGutterMarker::new("deleted", position(DiffSide::Original, 2)),
                DiffGutterMarker::new("added", position(DiffSide::Modified, 3)),
            ]);
        }
        if self.renderer {
            diff = diff.render_divider_marker(move |marker, _, _| {
                let clicked = clicked.clone();
                let target = marker.position().clone();
                let selector = format!("divider-{}", marker.id());
                div()
                    .id(marker.id().clone())
                    .size_3()
                    .debug_selector(move || selector.clone())
                    .on_click(move |_, _, _| clicked.borrow_mut().push(target.clone()))
            });
        }
        div().w(px(600.)).h(px(400.)).child(diff)
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
}

type Opened<'a> = (
    Entity<DiffState>,
    Entity<Host>,
    Rc<RefCell<Vec<DiffLinePosition>>>,
    &'a mut VisualTestContext,
);

fn open(cx: &mut TestAppContext, mode: DiffMode, markers: bool, renderer: bool) -> Opened<'_> {
    cx.update(crate::init);
    let state = cx.new(|cx| {
        DiffState::new(
            [fixture::modified(
                "file.txt",
                "one\ntwo\nthree\n",
                "one\nthree\nadded\n",
            )],
            cx,
        )
        .with_mode(mode)
    });
    let clicked = Rc::new(RefCell::new(Vec::new()));
    let (host, cx) = cx.add_window_view({
        let state = state.clone();
        let clicked = clicked.clone();
        move |_, _| Host {
            state,
            markers,
            renderer,
            clicked,
        }
    });
    for _ in 0..3 {
        draw(cx);
    }
    (state, host, clicked, cx)
}

fn content_width(state: &Entity<DiffState>, cx: &mut VisualTestContext) -> Pixels {
    cx.update(|_, cx| state.read(cx).content_width())
}

#[gpui::test]
fn divider_markers_on_one_sided_rows_activate_without_selecting(cx: &mut TestAppContext) {
    let (state, _, clicked, cx) = open(cx, DiffMode::Split, true, true);
    for (selector, expected) in [
        ("divider-deleted", position(DiffSide::Original, 2)),
        ("divider-added", position(DiffSide::Modified, 3)),
    ] {
        let marker = cx.debug_bounds(selector).expect(selector);
        cx.simulate_click(marker.center(), Default::default());
        draw(cx);
        assert_eq!(clicked.borrow().last(), Some(&expected));
    }
    assert!(cx.update(|_, cx| state.read(cx).selected_lines().is_none()));
}

#[gpui::test]
fn the_divider_column_only_widens_split_rows_with_a_renderer(cx: &mut TestAppContext) {
    let (state, host, _, cx) = open(cx, DiffMode::Split, true, false);
    // Markers without a renderer leave the split layout as it was.
    let plain = content_width(&state, cx);
    assert!(cx.debug_bounds("divider-deleted").is_none());

    let set_renderer = |cx: &mut VisualTestContext, renderer: bool| {
        host.update(cx, |host, cx| {
            host.renderer = renderer;
            cx.notify();
        });
        for _ in 0..3 {
            draw(cx);
        }
    };
    set_renderer(cx, true);
    let row_height = cx.update(|_, cx| state.read(cx).row_height()).unwrap();
    assert_eq!(content_width(&state, cx), plain + row_height);
    set_renderer(cx, false);
    assert_eq!(content_width(&state, cx), plain);

    // Unified views ignore divider markers.
    state.update(cx, |state, cx| state.set_mode(DiffMode::Unified, cx));
    for _ in 0..3 {
        draw(cx);
    }
    let unified = content_width(&state, cx);
    set_renderer(cx, true);
    assert_eq!(content_width(&state, cx), unified);
    assert!(cx.debug_bounds("divider-deleted").is_none());
}
