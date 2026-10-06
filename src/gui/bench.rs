//! Timing of the interface on the CPU renderer. Run with
//! `CARGO_PROFILE_DEV_OPT_LEVEL=2 cargo test bench_pages -- --ignored --nocapture`.
use super::*;
use iced::advanced::renderer::Headless;
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Renderer as _;
use iced::advanced::widget::Tree;
use iced::advanced::{mouse, renderer};
use iced::Rectangle;
use secblitz::engine::Outcome;
use std::time::Instant;

const SIZE: iced::Size = iced::Size::new(1100.0, 720.0);
const RUNS: u32 = 20;

fn report() -> Report {
    let statuses = ["attention", "compliant", "applied", "attention", "compliant"];
    Report {
        results: secblitz::hardening::all()
            .iter()
            .enumerate()
            .map(|(i, spec)| Outcome {
                id: spec.id.into(),
                title: spec.title.into(),
                status: statuses[i % statuses.len()].into(),
                detail: spec.description.into(),
                ..Outcome::default()
            })
            .collect(),
        ..Report::default()
    }
}

fn app() -> App {
    let (mut app, _) = App::new(Options {
        lang: Lang::En,
        broker: None,
        start: None,
    });
    let report = report();
    app.ctx.catalog.available = report.results.iter().map(|r| r.id.clone()).collect();
    app.ctx.checking = None;
    app.ctx.checked_at = Some(app::history::now());
    app.ctx.report = Some(Arc::new(report));
    let installed = (0..secblitz::debloat::catalog::CATALOG.len() as u16)
        .map(|index| secblitz::debloat::Installed {
            index,
            package: secblitz::debloat::catalog::CATALOG[index as usize]
                .family
                .into(),
            version: "1.0.0.0".into(),
        })
        .collect();
    let _ = app.update(Message::Debloat(debloat::Msg::Scanned(0, Ok(installed))));
    app
}

fn avg(f: &mut dyn FnMut()) -> f64 {
    f();
    let start = Instant::now();
    for _ in 0..RUNS {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / RUNS as f64
}

#[test]
fn the_toast_leaves_four_seconds_after_the_latest_one() {
    let mut app = app();
    drop(app.update(Message::Toast("First.".into(), Tone::Good)));
    drop(app.update(Message::Toast("Second.".into(), Tone::Good)));
    drop(app.update(Message::ToastExpire(1)));
    assert!(app.ctx.toast.is_some() && !app.toast_leaving, "an old timer must not close a newer toast");
    drop(app.update(Message::ToastExpire(2)));
    assert!(app.toast_leaving || app.ctx.toast.is_none());
}

#[test]
fn checking_again_shows_the_whole_checking_screen_then_hands_off() {
    let _motion = widgets::anim::forced::set(false);
    let mut app = app();
    drop(app.update(Message::CheckNow));
    assert!(home::fills_window(&app.ctx) && fixes::fills_window(&app.ctx));
    let done = Ok(app.ctx.report.clone().expect("a report"));
    drop(app.update(Message::Worker(worker::Event::Checked(done))));
    assert!(app.ctx.finishing && app.handoff.is_some(), "the result waits for the screen to settle");
    drop(app.update(Message::Navigate(Page::Tools)));
    assert!(!app.ctx.finishing && app.ctx.checking.is_none());
}

#[test]
fn the_check_after_a_fix_keeps_the_results_on_screen() {
    let mut app = app();
    app.ctx.checking = Some(CheckProgress {
        phase: Some(worker::Phase::Verifying),
        items: Vec::new(),
    });
    assert!(!home::fills_window(&app.ctx) && !fixes::fills_window(&app.ctx));
}

fn type_into_protection(app: &mut App, text: &str) {
    drop(app.update(Message::Fixes(fixes::Msg::Search(text.into()))));
}

fn shown_on_protection(app: &App) -> Vec<String> {
    fixes::visible_rows(&app.fixes, &app.ctx)
}

#[test]
fn typing_in_protection_keeps_only_matching_rows_in_their_normal_order() {
    let mut app = app();
    app.page = Page::Fixes;
    let all = shown_on_protection(&app);
    assert!(all.len() > 20, "{}", all.len());
    type_into_protection(&mut app, "firewall");
    let some = shown_on_protection(&app);
    assert!(!some.is_empty() && some.len() < all.len(), "{} of {}", some.len(), all.len());
    let in_order: Vec<_> = all.iter().filter(|key| some.contains(key)).collect();
    assert_eq!(in_order, some.iter().collect::<Vec<_>>(), "rows must not jump around");
    type_into_protection(&mut app, "firewall zzzqqq");
    assert!(shown_on_protection(&app).is_empty(), "every word has to match");
    type_into_protection(&mut app, "  ");
    assert_eq!(shown_on_protection(&app), all, "blank means no filter");
    drop(app.view());
}

#[test]
fn a_row_is_found_by_its_id_and_by_a_typo() {
    let mut app = app();
    let all = shown_on_protection(&app);
    let id = all[0].clone();
    type_into_protection(&mut app, &id);
    assert!(shown_on_protection(&app).contains(&id));
    type_into_protection(&mut app, "frewall");
    assert!(!shown_on_protection(&app).is_empty());
}

#[test]
fn english_words_still_find_rows_while_the_app_is_in_another_language() {
    let mut app = app();
    type_into_protection(&mut app, "firewall");
    let english = shown_on_protection(&app);
    app.ctx.lang = Lang::De;
    drop(app.update(Message::Fixes(fixes::Msg::Search("firewall".into()))));
    let german = shown_on_protection(&app);
    assert!(!english.is_empty());
    assert!(english.iter().all(|key| german.contains(key)), "{english:?} vs {german:?}");
    drop(app.view());
}

#[test]
fn a_search_with_no_match_shows_the_calm_page_and_clearing_brings_everything_back() {
    let mut app = app();
    app.page = Page::Fixes;
    let all = shown_on_protection(&app);
    type_into_protection(&mut app, "qqqzzzxxx");
    assert!(shown_on_protection(&app).is_empty());
    drop(app.view());
    drop(app.update(Message::Fixes(fixes::Msg::ClearSearch)));
    assert_eq!(shown_on_protection(&app), all);
    type_into_protection(&mut app, "firewall");
    drop(app.update(Message::Escape));
    assert_eq!(shown_on_protection(&app), all, "Escape clears the search");
}

#[test]
fn select_all_in_protection_takes_only_the_rows_shown_and_hidden_choices_stay() {
    let mut app = app();
    let none = |app: &mut App| drop(app.update(Message::Fixes(fixes::Msg::SelectNone)));
    none(&mut app);
    assert!(fixes::selected_ids(&app.fixes).is_empty());
    type_into_protection(&mut app, "firewall");
    let first = fixes::shown_fixable(&app.fixes, &app.ctx);
    assert!(!first.is_empty(), "the test report needs a fixable firewall row");
    drop(app.update(Message::Fixes(fixes::Msg::SelectAll)));
    let mut expected = first.clone();
    expected.sort();
    assert_eq!(fixes::selected_ids(&app.fixes), expected);

    let others: Vec<String> = fixes::shown_fixable(&app.fixes, &app.ctx);
    type_into_protection(&mut app, "");
    let every: Vec<String> = fixes::shown_fixable(&app.fixes, &app.ctx);
    assert!(every.len() > others.len(), "the unfiltered list has more fixable rows");
    let outside = every.iter().find(|id| !first.contains(id)).unwrap().clone();
    type_into_protection(&mut app, &outside);
    assert!(!fixes::shown_fixable(&app.fixes, &app.ctx).iter().any(|id| first.contains(id)));
    assert_eq!(fixes::selected_ids(&app.fixes), expected, "typing never changes the choice");
    drop(app.update(Message::Fixes(fixes::Msg::SelectAll)));
    let selected = fixes::selected_ids(&app.fixes);
    assert!(selected.contains(&outside) && first.iter().all(|id| selected.contains(id)));
    drop(app.update(Message::Fixes(fixes::Msg::SelectNone)));
    let left = fixes::selected_ids(&app.fixes);
    assert!(!left.contains(&outside) && first.iter().all(|id| left.contains(id)), "none only clears what is shown");
    drop(app.view());
    type_into_protection(&mut app, "");
    drop(app.update(Message::Fixes(fixes::Msg::SelectAll)));
    assert_eq!(fixes::selected_ids(&app.fixes).len(), every.len());
}

fn with_report(app: &mut App, edit: impl FnOnce(&mut Report)) {
    let mut r = report();
    edit(&mut r);
    app.ctx.catalog.available = r.results.iter().map(|o| o.id.clone()).collect();
    app.ctx.checked_at = Some(app.ctx.checked_at.unwrap_or(0) + 1);
    app.ctx.report = Some(Arc::new(r));
}

/// Marks the first `n` protected settings as ones Secblitz changed.
fn mark_undoable(r: &mut Report, n: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for o in r.results.iter_mut().filter(|o| o.status == CheckStatus::Compliant) {
        if ids.len() == n {
            break;
        }
        o.undoable = true;
        ids.push(o.id.clone());
    }
    ids
}

fn app_with_undoable(n: usize) -> (App, Vec<String>) {
    let mut app = app();
    let mut ids = Vec::new();
    with_report(&mut app, |r| ids = mark_undoable(r, n));
    assert_eq!(ids.len(), n);
    (app, ids)
}

fn lays_out(app: &mut App, renderer: &iced::Renderer) {
    app.enter_t = 1.0;
    let mut element = app.view();
    let mut tree = Tree::new(&element);
    let node = element
        .as_widget_mut()
        .layout(&mut tree, renderer, &Limits::new(iced::Size::ZERO, SIZE));
    assert!(node.size().width > 0.0 && node.size().height > 0.0);
}

fn restored(id: &str) -> Outcome {
    Outcome {
        id: id.into(),
        status: CheckStatus::Restored,
        ..Outcome::default()
    }
}

#[test]
fn settings_secblitz_changed_come_first_and_nothing_starts_chosen() {
    let (mut app, ids) = app_with_undoable(3);
    app.page = Page::Fixes;
    assert!(fixes::undo_selected_ids(&app.fixes).is_empty());
    assert_eq!(fixes::all_undoable(&app.fixes, &app.ctx).len(), 3);
    let rows = fixes::visible_rows(&app.fixes, &app.ctx);
    let at = |id: &String| rows.iter().position(|k| k == id).expect("shown");
    let plain: Vec<String> = app
        .ctx
        .report
        .as_deref()
        .unwrap()
        .results
        .iter()
        .filter(|o| o.status == CheckStatus::Compliant && !o.undoable)
        .map(|o| o.id.clone())
        .collect();
    assert!(!plain.is_empty());
    let last_changed = ids.iter().map(at).max().unwrap();
    assert!(plain.iter().all(|id| at(id) > last_changed), "changed settings sort first");
    drop(app.update(Message::Fixes(fixes::Msg::ToggleUndo(plain[0].clone()))));
    assert!(fixes::undo_selected_ids(&app.fixes).is_empty(), "only changed settings can be chosen");
    drop(app.update(Message::Fixes(fixes::Msg::ToggleUndo(ids[0].clone()))));
    assert_eq!(fixes::undo_selected_ids(&app.fixes), vec![ids[0].clone()]);
    drop(app.update(Message::Fixes(fixes::Msg::ToggleUndo(ids[0].clone()))));
    assert!(fixes::undo_selected_ids(&app.fixes).is_empty());
    drop(app.update(Message::Fixes(fixes::Msg::FocusUndo)));
    assert!(fixes::open_protected(&app.fixes));
    drop(app.view());
}

#[test]
fn a_chosen_selection_acts_on_the_rows_shown_and_is_pruned_by_a_new_check() {
    let (mut app, ids) = app_with_undoable(3);
    drop(app.update(Message::Fixes(fixes::Msg::UndoSelectAll)));
    assert_eq!(fixes::undo_selected_ids(&app.fixes).len(), 3);
    drop(app.update(Message::Fixes(fixes::Msg::UndoSelectNone)));
    assert!(fixes::undo_selected_ids(&app.fixes).is_empty());

    type_into_protection(&mut app, &ids[0]);
    let shown = fixes::shown_undoable(&app.fixes, &app.ctx);
    assert!(shown.contains(&ids[0]) && shown.len() < 3, "{shown:?}");
    drop(app.update(Message::Fixes(fixes::Msg::UndoSelectAll)));
    let mut want = shown.clone();
    want.sort();
    assert_eq!(fixes::undo_selected_ids(&app.fixes), want, "only what the search shows");
    type_into_protection(&mut app, "");
    drop(app.update(Message::Fixes(fixes::Msg::UndoSelectAll)));
    assert_eq!(fixes::undo_selected_ids(&app.fixes).len(), 3);
    type_into_protection(&mut app, &ids[0]);
    drop(app.update(Message::Fixes(fixes::Msg::UndoSelectNone)));
    let left = fixes::undo_selected_ids(&app.fixes);
    assert!(!left.contains(&ids[0]) && left.len() == 3 - shown.len(), "none only clears what is shown");
    drop(app.view());
    type_into_protection(&mut app, "");

    let keep = left[0].clone();
    with_report(&mut app, |r| {
        for o in &mut r.results {
            o.undoable = o.id == keep;
        }
    });
    drop(app.update(Message::Fixes(fixes::Msg::Search(String::new()))));
    assert_eq!(fixes::undo_selected_ids(&app.fixes), vec![keep], "a new check keeps only what can still be put back");
    with_report(&mut app, |_| {});
    drop(app.update(Message::Fixes(fixes::Msg::Search(String::new()))));
    assert!(fixes::undo_selected_ids(&app.fixes).is_empty());
}

#[test]
fn putting_back_chosen_settings_goes_from_review_to_working_to_a_calm_result() {
    let _motion = widgets::anim::forced::set(false);
    let (mut app, ids) = app_with_undoable(3);
    let stranger = "not.changed.by.us".to_owned();
    drop(app.update(Message::ReviewUndoSome(vec![
        ids[1].clone(),
        stranger,
        ids[1].clone(),
        ids[0].clone(),
    ])));
    assert_eq!(
        fixflow::review(&app.fix),
        Some((vec![ids[1].clone(), ids[0].clone()], true)),
        "only settings Secblitz changed, once each, in the order chosen"
    );
    assert_eq!(fixflow::plan_ids(&app.fix), vec![ids[1].clone(), ids[0].clone()]);
    assert!(!fixflow::added_together(&app.fix));
    drop(app.view());

    drop(app.update(Message::Fix(fixflow::Msg::Cancel)));
    assert_eq!(fixflow::stage_name(&app.fix), "closed");
    drop(app.update(Message::ReviewUndoSome(vec![ids[0].clone()])));
    drop(app.update(Message::Escape));
    assert_eq!(fixflow::stage_name(&app.fix), "closed");

    app.ctx.busy = true;
    drop(app.update(Message::ReviewUndoSome(vec![ids[0].clone()])));
    assert_eq!(fixflow::stage_name(&app.fix), "closed", "nothing opens while another job runs");
    app.ctx.busy = false;
    app.ctx.checking = Some(CheckProgress::default());
    drop(app.update(Message::ReviewUndoSome(vec![ids[0].clone()])));
    assert_eq!(fixflow::stage_name(&app.fix), "closed", "nor while a check runs");
    app.ctx.checking = None;

    drop(app.update(Message::ReviewUndoSome(vec![ids[0].clone(), ids[1].clone()])));
    drop(app.update(Message::Fix(fixflow::Msg::Confirm)));
    assert_eq!(fixflow::stage_name(&app.fix), "review", "a pre-flight check comes first");
    drop(app.update(Message::Worker(worker::Event::Preflight {
        undo: true,
        result: Ok(()),
    })));
    assert_eq!(fixflow::stage_name(&app.fix), "working");
    assert!(app.ctx.busy);
    drop(app.view());
    for id in [&ids[0], &ids[1]] {
        drop(app.update(Message::Worker(worker::Event::Progress {
            phase: worker::Phase::Undoing,
            id: id.clone(),
            status: "restored".into(),
        })));
    }
    drop(app.view());

    let result = Report {
        results: vec![
            restored(&ids[0]),
            Outcome {
                id: ids[1].clone(),
                status: CheckStatus::Conflict,
                ..Outcome::default()
            },
        ],
        ..Report::default()
    };
    drop(app.update(Message::Worker(worker::Event::Undone {
        chosen: vec![ids[0].clone(), ids[1].clone()],
        result: Ok(Arc::new(result)),
        verify: Ok(app.ctx.report.clone().unwrap()),
    })));
    assert!(!app.ctx.busy);
    assert_eq!(fixflow::stage_name(&app.fix), "result");
    let summary = fixflow::result_summary(&app.fix).expect("a result");
    assert_eq!(summary.done, vec![ids[0].clone()]);
    assert_eq!(summary.not_done, vec![(ids[1].clone(), app::flow::REASON_LEFT_AS_IS.to_owned())]);
    assert!(!summary.less_protected.is_empty());
    drop(app.view());
    drop(app.update(Message::Fix(fixflow::Msg::Done)));
    assert_eq!(fixflow::stage_name(&app.fix), "closed");
}

#[test]
fn a_protection_another_one_needs_is_added_and_said_so() {
    use secblitz::vbs::{MEMORY_INTEGRITY, STACK_PROTECTION};
    let mut app = app();
    with_report(&mut app, |r| {
        for o in &mut r.results {
            if o.id == MEMORY_INTEGRITY || o.id == STACK_PROTECTION {
                o.status = CheckStatus::Compliant;
                o.undoable = true;
            }
        }
    });
    drop(app.update(Message::ReviewUndoSome(vec![MEMORY_INTEGRITY.into()])));
    assert_eq!(
        fixflow::plan_ids(&app.fix),
        vec![STACK_PROTECTION.to_owned(), MEMORY_INTEGRITY.to_owned()]
    );
    assert!(fixflow::added_together(&app.fix));
    let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        theme::REGULAR,
        14.0.into(),
        Some("tiny-skia"),
    ))
    .expect("tiny-skia renderer");
    for lang in [Lang::En, Lang::De] {
        app.ctx.lang = lang;
        lays_out(&mut app, &renderer);
    }
    drop(app.update(Message::Fix(fixflow::Msg::Cancel)));
    drop(app.update(Message::ReviewUndoSome(vec![STACK_PROTECTION.into()])));
    assert!(!fixflow::added_together(&app.fix));
}

#[test]
fn the_last_fixes_come_from_the_check_and_follow_a_partial_undo() {
    let (mut app, ids) = app_with_undoable(3);
    with_report(&mut app, |r| r.undo_next = vec![ids[0].clone(), ids[1].clone()]);
    drop(app.update(Message::ReviewUndo));
    assert_eq!(fixflow::review(&app.fix), Some((Vec::new(), true)));
    assert_eq!(fixflow::plan_ids(&app.fix), vec![ids[0].clone(), ids[1].clone()]);
    drop(app.update(Message::Fix(fixflow::Msg::Cancel)));
    with_report(&mut app, |r| r.undo_next = vec![ids[1].clone()]);
    drop(app.update(Message::ReviewUndo));
    assert_eq!(fixflow::plan_ids(&app.fix), vec![ids[1].clone()], "no stale rows");
    drop(app.update(Message::Fix(fixflow::Msg::Cancel)));
    with_report(&mut app, |_| {});
    drop(app.update(Message::ReviewUndo));
    assert!(fixflow::plan_ids(&app.fix).is_empty());
    drop(app.view());
}

#[test]
fn the_history_row_opens_the_protection_page_on_the_changed_settings() {
    let (mut app, _) = app_with_undoable(2);
    drop(app.update(Message::Navigate(Page::History)));
    assert_eq!(app.page, Page::History);
    drop(app.update(Message::PutBackChosen));
    assert_eq!(app.page, Page::Fixes);
    assert!(fixes::open_protected(&app.fixes));
}

#[test]
fn the_protection_page_lays_out_with_changed_settings_chosen_and_while_busy() {
    let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        theme::REGULAR,
        14.0.into(),
        Some("tiny-skia"),
    ))
    .expect("tiny-skia renderer");
    let (mut app, ids) = app_with_undoable(3);
    app.page = Page::Fixes;
    drop(app.update(Message::Fixes(fixes::Msg::FocusUndo)));
    lays_out(&mut app, &renderer);
    drop(app.update(Message::Fixes(fixes::Msg::ToggleUndo(ids[0].clone()))));
    lays_out(&mut app, &renderer);
    app.ctx.busy = true;
    lays_out(&mut app, &renderer);
    app.ctx.busy = false;
    for typed in [ids[0].as_str(), "qqqzzz"] {
        type_into_protection(&mut app, typed);
        lays_out(&mut app, &renderer);
    }
    app.ctx.lang = Lang::Fr;
    type_into_protection(&mut app, "");
    lays_out(&mut app, &renderer);
}

fn type_into_clean_up(app: &mut App, text: &str) {
    drop(app.update(Message::Debloat(debloat::Msg::Search(text.into()))));
}

#[test]
fn typing_in_clean_up_apps_hides_other_apps_and_finds_typos_and_package_names() {
    let mut app = app();
    app.page = Page::Debloat;
    let all = debloat::visible_apps(&app.debloat, &app.ctx);
    assert!(all.len() > 20);
    type_into_clean_up(&mut app, "xbox");
    let xbox = debloat::visible_apps(&app.debloat, &app.ctx);
    assert!(!xbox.is_empty() && xbox.len() < all.len());
    drop(app.view());
    type_into_clean_up(&mut app, "solitare");
    let names: Vec<&str> = debloat::visible_apps(&app.debloat, &app.ctx)
        .into_iter()
        .map(|i| secblitz::debloat::catalog()[i as usize].name)
        .collect();
    assert_eq!(names, ["Solitaire games"]);
    type_into_clean_up(&mut app, "GamingOverlay");
    let by_package = debloat::visible_apps(&app.debloat, &app.ctx);
    assert!(by_package.iter().any(|i| secblitz::debloat::catalog()[*i as usize].name == "Game Bar"));
    type_into_clean_up(&mut app, "qqqzzz");
    assert!(debloat::visible_apps(&app.debloat, &app.ctx).is_empty());
    drop(app.view());
    drop(app.update(Message::Escape));
    assert_eq!(debloat::visible_apps(&app.debloat, &app.ctx), all, "Escape clears the search");
}

#[test]
fn choosing_a_whole_group_while_searching_takes_only_the_apps_shown() {
    let mut app = app();
    type_into_clean_up(&mut app, "xbox");
    let shown = debloat::visible_apps(&app.debloat, &app.ctx);
    let group = secblitz::debloat::catalog()[shown[0] as usize].group;
    drop(app.update(Message::Debloat(debloat::Msg::ToggleGroup(group))));
    let chosen = debloat::selected_apps(&app.debloat);
    let in_group: Vec<u16> = shown
        .iter()
        .copied()
        .filter(|i| secblitz::debloat::catalog()[*i as usize].group == group)
        .collect();
    assert_eq!(chosen, in_group);
    type_into_clean_up(&mut app, "weather");
    assert_eq!(debloat::selected_apps(&app.debloat), in_group, "hidden choices stay");
    drop(app.update(Message::Debloat(debloat::Msg::ToggleGroup(secblitz::debloat::catalog()[0].group))));
    let after = debloat::selected_apps(&app.debloat);
    assert!(in_group.iter().all(|i| after.contains(i)) && after.len() > in_group.len());
    drop(app.update(Message::Debloat(debloat::Msg::ClearSearch)));
    drop(app.view());
}

#[test]
fn both_pages_lay_out_with_a_search_active_and_with_nothing_found() {
    let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        theme::REGULAR,
        14.0.into(),
        Some("tiny-skia"),
    ))
    .expect("tiny-skia renderer");
    let mut app = app();
    for (page, found, nothing) in [
        (Page::Fixes, "firewall", "qqqzzz"),
        (Page::Debloat, "xbox", "qqqzzz"),
    ] {
        app.page = page;
        app.enter_t = 1.0;
        for typed in ["", found, nothing] {
            match page {
                Page::Fixes => type_into_protection(&mut app, typed),
                _ => type_into_clean_up(&mut app, typed),
            }
            let mut element = app.view();
            let mut tree = Tree::new(&element);
            let node = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &Limits::new(iced::Size::ZERO, SIZE));
            assert!(node.size().width > 0.0 && node.size().height > 0.0, "{page:?} {typed:?}");
        }
    }
}

#[test]
#[ignore]
fn bench_pages() {
    let mut renderer = iced::futures::executor::block_on(
        <iced::Renderer as Headless>::new(theme::REGULAR, 14.0.into(), Some("tiny-skia")),
    )
    .expect("tiny-skia renderer");
    let mut app = app();
    let theme = app.ctx.palette.theme();
    let style = renderer::Style {
        text_color: theme.palette().text,
    };
    renderer.reset(Rectangle::with_size(SIZE));
    let empty = avg(&mut || {
        let _ = renderer.screenshot(iced::Size::new(1100, 720), 1.0, iced::Color::WHITE);
    });
    println!("empty frame {empty:.2}");
    println!("page        view   layout  draw   raster (ms, average of {RUNS})");
    for page in Page::ALL {
        app.page = page;
        app.enter_t = 1.0;
        let view = avg(&mut || drop(app.view()));
        let layout_ms = avg(&mut || {
            let mut element = app.view();
            let mut tree = Tree::new(&element);
            let _ = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &Limits::new(iced::Size::ZERO, SIZE));
        });
        let mut element = app.view();
        let mut tree = Tree::new(&element);
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &Limits::new(iced::Size::ZERO, SIZE));
        let viewport = Rectangle::with_size(SIZE);
        let draw = avg(&mut || {
            renderer.reset(viewport);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &style,
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &viewport,
            );
        });
        let raster = avg(&mut || {
            let _ = renderer.screenshot(
                iced::Size::new(SIZE.width as u32, SIZE.height as u32),
                1.0,
                iced::Color::WHITE,
            );
        });
        println!("{page:<10?} {view:6.2} {:6.2} {draw:6.2} {raster:6.2}", layout_ms - view);
    }
    println!("update handlers (ms, average of {RUNS})");
    for page in Page::ALL {
        let t = avg(&mut || {
            app.page = Page::Home;
            drop(app.update(Message::Navigate(page)));
        });
        println!("navigate to {page:<10?} {t:6.3}");
    }
    let checked = Arc::new(report());
    let t = avg(&mut || {
        drop(app.update(Message::Worker(worker::Event::Checked(Ok(checked.clone())))));
    });
    println!("check finished      {t:6.3}");
    let t = avg(&mut || {
        drop(app.update(Message::Toast("Saved.".into(), Tone::Good)));
    });
    println!("toast               {t:6.3}");
}
