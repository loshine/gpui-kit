use gpui::{
    AppContext as _, Bounds, Context, Entity, IntoElement, ParentElement as _, Pixels, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::{Diff, DiffMode, DiffState, document::fixture};

struct Host {
    state: Entity<DiffState>,
    width: Pixels,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(self.width)
            .h(px(300.))
            .child(Diff::new(&self.state).size_full())
    }
}

fn draw(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }
}

/// The first line is short on the left and far wider than a column on the right.
fn open(cx: &mut TestAppContext) -> (Entity<DiffState>, Entity<Host>, &mut VisualTestContext) {
    cx.update(crate::init);
    let long = "x".repeat(400);
    let state = cx.new(|cx| {
        DiffState::new(
            [fixture::modified(
                "file.txt",
                "short\nsame\n",
                &format!("{long}\nsame\n"),
            )],
            cx,
        )
        .with_mode(DiffMode::Split)
    });
    let (host, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, _| Host {
            state,
            width: px(600.),
        }
    });
    draw(cx);
    (state, host, cx)
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector).expect(selector)
}

fn assert_equal_columns(cx: &mut VisualTestContext) {
    let old = bounds(cx, "split-code-old-0");
    let new = bounds(cx, "split-code-new-0");
    // The columns are equal; the original one gives a pixel to the border
    // between them.
    let difference = (old.size.width - new.size.width).abs();
    assert!(difference <= px(1.), "{old:?} {new:?}");
    assert!(old.right() <= new.left(), "{old:?} {new:?}");
}

#[gpui::test]
fn split_columns_take_half_each_and_clip_long_lines(cx: &mut TestAppContext) {
    let (state, _, cx) = open(cx);
    assert_equal_columns(cx);
    let new = bounds(cx, "split-code-new-0");
    assert!(new.right() <= px(600.), "{new:?}");
    // The long line overflows its column, so the code can scroll sideways,
    // by no more than that overflow.
    let range = cx.update(|_, cx| state.read(cx).horizontal_scroll().max_offset().x);
    assert!(range > px(0.) && range < px(400.) * 8., "{range:?}");
}

#[gpui::test]
fn both_split_columns_scroll_sideways_together(cx: &mut TestAppContext) {
    let (state, _, cx) = open(cx);
    let offsets = |cx: &mut VisualTestContext| {
        ["old", "new"].map(|side| {
            let code = match side {
                "old" => bounds(cx, "split-code-old-0"),
                _ => bounds(cx, "split-code-new-0"),
            };
            let source = match side {
                "old" => bounds(cx, "split-source-old-0"),
                _ => bounds(cx, "split-source-new-0"),
            };
            (code.left(), source.left() - code.left())
        })
    };
    let before = offsets(cx);
    cx.update(|_, cx| {
        state
            .read(cx)
            .horizontal_scroll()
            .set_offset(point(px(-120.), px(0.)))
    });
    draw(cx);
    let after = offsets(cx);
    for (before, after) in before.iter().zip(&after) {
        // The columns stay put and their code moves by the same distance.
        assert_eq!(after.0, before.0);
        assert_eq!(after.1, before.1 - px(120.));
    }
    assert_equal_columns(cx);
}

#[gpui::test]
fn split_columns_stay_equal_when_the_window_narrows(cx: &mut TestAppContext) {
    let (_, host, cx) = open(cx);
    let wide = bounds(cx, "split-code-old-0").size.width;
    host.update(cx, |host, cx| {
        host.width = px(400.);
        cx.notify();
    });
    draw(cx);
    assert_equal_columns(cx);
    assert!(bounds(cx, "split-code-old-0").size.width < wide);
}
