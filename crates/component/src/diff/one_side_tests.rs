use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};

use super::{Diff, DiffFile, DiffMode, DiffSide, DiffState};

const ADDED: &str = "diff --git a/added.txt b/added.txt\n\
new file mode 100644\n\
index 0000000..de98044\n\
--- /dev/null\n\
+++ b/added.txt\n\
@@ -0,0 +1,3 @@\n\
+a\n\
+b\n\
+c\n";

const DELETED: &str = "diff --git a/deleted.txt b/deleted.txt\n\
deleted file mode 100644\n\
index de98044..0000000\n\
--- a/deleted.txt\n\
+++ /dev/null\n\
@@ -1,3 +0,0 @@\n\
-a\n\
-b\n\
-c\n";

const EMPTY_ADDED: &str = "diff --git a/empty.txt b/empty.txt\n\
new file mode 100644\n\
index 0000000..e69de29\n";

const EMPTY_DELETED: &str = "diff --git a/empty.txt b/empty.txt\n\
deleted file mode 100644\n\
index e69de29..0000000\n";

struct Host {
    state: Entity<DiffState>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(600.))
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

fn open<'a>(
    cx: &'a mut TestAppContext,
    patch: &str,
    mode: DiffMode,
    one_side: bool,
) -> (Entity<DiffState>, &'a mut VisualTestContext) {
    cx.update(crate::init);
    let files = DiffFile::parse(patch).unwrap();
    let state = cx.new(|cx| {
        DiffState::new(files, cx)
            .with_mode(mode)
            .with_context_lines(None)
            .with_one_side_files(one_side)
    });
    let (_, cx) = cx.add_window_view({
        let state = state.clone();
        move |_, _| Host { state }
    });
    draw(cx);
    (state, cx)
}

fn has(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

fn source_x(cx: &mut VisualTestContext, selector: &'static str) -> gpui::Pixels {
    cx.debug_bounds(selector).expect(selector).origin.x
}

/// One column with the existing side's numbers, in both modes; the code
/// starts one gutter further left than beside an empty column.
fn assert_one_column(cx: &mut TestAppContext, patch: &str, side: DiffSide) {
    let (gutter, other_gutter, source, split_code) = match side {
        DiffSide::Modified => (
            "gutter-new-3",
            "gutter-old-1",
            "source-new-0",
            "split-code-new-0",
        ),
        DiffSide::Original => (
            "gutter-old-3",
            "gutter-new-1",
            "source-old-0",
            "split-code-old-0",
        ),
    };
    let mut xs = Vec::new();
    for one_side in [true, false] {
        for mode in [DiffMode::Split, DiffMode::Unified] {
            let (_, cx) = open(cx, patch, mode, one_side);
            assert!(has(cx, gutter), "{mode:?} {one_side}");
            assert!(!has(cx, other_gutter), "{mode:?} {one_side}");
            // Split columns clip their code; one column does not.
            if mode == DiffMode::Split {
                assert_eq!(has(cx, split_code), !one_side, "{one_side}");
            }
            xs.push((one_side, mode, source_x(cx, source)));
        }
    }
    let x = |one_side, mode| {
        xs.iter()
            .find(|(o, m, _)| *o == one_side && *m == mode)
            .unwrap()
            .2
    };
    // One column: the same place in both modes.
    assert_eq!(x(true, DiffMode::Split), x(true, DiffMode::Unified));
    // Unified beside an empty column carries a second, empty gutter.
    assert!(x(true, DiffMode::Unified) < x(false, DiffMode::Unified));
}

#[gpui::test]
fn added_file_shows_one_column(cx: &mut TestAppContext) {
    assert_one_column(cx, ADDED, DiffSide::Modified);
}

#[gpui::test]
fn deleted_file_shows_one_column(cx: &mut TestAppContext) {
    assert_one_column(cx, DELETED, DiffSide::Original);
}

#[gpui::test]
fn mode_change_keeps_one_column(cx: &mut TestAppContext) {
    let (state, cx) = open(cx, ADDED, DiffMode::Split, true);
    let split = source_x(cx, "source-new-0");
    state.update(cx, |state, cx| state.set_mode(DiffMode::Unified, cx));
    draw(cx);
    assert_eq!(source_x(cx, "source-new-0"), split);
    state.update(cx, |state, cx| state.set_mode(DiffMode::Split, cx));
    draw(cx);
    assert_eq!(source_x(cx, "source-new-0"), split);
    assert!(!has(cx, "split-code-new-0"));
}

#[gpui::test]
fn switch_changes_columns_after_creation(cx: &mut TestAppContext) {
    let (state, cx) = open(cx, ADDED, DiffMode::Split, false);
    assert!(has(cx, "split-code-new-0"));
    state.update(cx, |state, cx| state.set_one_side_files(true, cx));
    draw(cx);
    assert!(state.read_with(cx, |state, _| state.one_side_files()));
    assert!(!has(cx, "split-code-new-0"));
}

#[gpui::test]
fn empty_one_side_files_show_one_empty_line(cx: &mut TestAppContext) {
    for (patch, line, other) in [
        (EMPTY_ADDED, "empty-line-new", "empty-line-old"),
        (EMPTY_DELETED, "empty-line-old", "empty-line-new"),
    ] {
        for mode in [DiffMode::Split, DiffMode::Unified] {
            let (_, cx) = open(cx, patch, mode, true);
            assert!(has(cx, line), "{line} {mode:?}");
            assert!(!has(cx, other), "{line} {mode:?}");
            let (_, cx) = open(cx, patch, mode, false);
            assert!(!has(cx, line), "{line} {mode:?}");
        }
    }
}

#[gpui::test]
fn next_change_reaches_the_one_side(cx: &mut TestAppContext) {
    for (patch, side) in [(ADDED, DiffSide::Modified), (DELETED, DiffSide::Original)] {
        for mode in [DiffMode::Split, DiffMode::Unified] {
            let (state, cx) = open(cx, patch, mode, true);
            let position = state
                .update(cx, |state, cx| state.step_change(true, None, cx))
                .expect("a change");
            assert_eq!((position.side(), position.line()), (side, 1), "{mode:?}");
            assert!(!state.read_with(cx, |state, _| state.has_change(true, Some(&position))));
        }
    }
}
