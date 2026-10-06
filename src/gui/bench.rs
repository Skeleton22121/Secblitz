//! Timing of the interface on the CPU renderer. Run with
//! `CARGO_PROFILE_DEV_OPT_LEVEL=2 cargo test bench_pages -- --ignored --nocapture`.
use super::*;
use iced::advanced::renderer::Headless;
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Renderer as _;
use iced::advanced::widget::Tree;
use iced::advanced::{mouse, renderer};
use iced::Rectangle;
use crate::app::settings::ToolsSection;
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

fn tools_msg(app: &mut App, msg: tools::Msg) {
    drop(app.update(Message::Tools(msg)));
}

fn toggle_tools_section(app: &mut App, section: ToolsSection) {
    tools_msg(app, tools::Msg::ToggleSection(section));
}

fn tools_status(app: &App, section: ToolsSection) -> Option<tools::Status> {
    tools::section_status(&app.tools, &app.ctx, section)
}

const TOOLS_SECTIONS: [ToolsSection; 6] = [
    ToolsSection::Virus,
    ToolsSection::Repair,
    ToolsSection::Passwords,
    ToolsSection::Account,
    ToolsSection::Apps,
    ToolsSection::Windows,
];

#[test]
fn the_tools_page_starts_with_pc_health_tips_then_the_closed_sections() {
    assert_eq!(tools::PAGE_ORDER[0], tools::Block::Tips);
    let rest: Vec<_> = tools::PAGE_ORDER[1..]
        .iter()
        .map(|block| match block {
            tools::Block::Section(section) => *section,
            tools::Block::Tips => panic!("tips appear once, at the top"),
        })
        .collect();
    assert_eq!(rest, TOOLS_SECTIONS);
    let mut app = app();
    assert!(TOOLS_SECTIONS.iter().all(|s| !app.ctx.prefs.tools_section_open(*s)));
    app.page = Page::Tools;
    drop(app.view());
}

#[test]
fn a_tools_section_opens_and_closes_without_touching_the_others() {
    let mut app = app();
    toggle_tools_section(&mut app, ToolsSection::Repair);
    assert!(app.ctx.prefs.tools_section_open(ToolsSection::Repair));
    assert!(TOOLS_SECTIONS
        .iter()
        .filter(|s| **s != ToolsSection::Repair)
        .all(|s| !app.ctx.prefs.tools_section_open(*s)));
    toggle_tools_section(&mut app, ToolsSection::Windows);
    toggle_tools_section(&mut app, ToolsSection::Repair);
    assert!(!app.ctx.prefs.tools_section_open(ToolsSection::Repair));
    assert!(app.ctx.prefs.tools_section_open(ToolsSection::Windows));
    app.page = Page::Tools;
    drop(app.view());
}

#[test]
fn tools_sections_stay_as_left_after_leaving_the_page_and_after_a_restart() {
    let mut app = app();
    toggle_tools_section(&mut app, ToolsSection::Apps);
    toggle_tools_section(&mut app, ToolsSection::Virus);
    drop(app.update(Message::Navigate(Page::Tools)));
    drop(app.update(Message::Navigate(Page::Home)));
    drop(app.update(Message::Navigate(Page::Fixes)));
    drop(app.update(Message::Navigate(Page::Tools)));
    assert!(app.ctx.prefs.tools_section_open(ToolsSection::Apps));
    assert!(app.ctx.prefs.tools_section_open(ToolsSection::Virus));
    assert!(!app.ctx.prefs.tools_section_open(ToolsSection::Passwords));
    let saved = serde_json::to_vec(&app.ctx.prefs).expect("prefs serialize");
    let restarted = app::settings::parse(&saved);
    assert_eq!(restarted.tools_open, app.ctx.prefs.tools_open);
    assert!(restarted.tools_section_open(ToolsSection::Apps));
}

#[test]
fn a_closed_tools_section_says_what_is_running_or_finished_inside() {
    let mut app = app();
    assert!(TOOLS_SECTIONS.iter().all(|s| tools_status(&app, *s).is_none()));

    tools_msg(&mut app, tools::Msg::ScanDone(Ok(())));
    let scan = tools_status(&app, ToolsSection::Virus).expect("a finished scan is told");
    assert_eq!((scan.tone, scan.text.as_str()), (Tone::Good, "Scan started"));

    tools_msg(&mut app, tools::Msg::DefenderDone(Err("offline".into())));
    let defender = tools_status(&app, ToolsSection::Virus).expect("a failed update is told");
    assert_eq!(defender.tone, Tone::Warn, "what needs attention wins over what went well");
    assert_eq!(defender.text, "We couldn't update right now");

    let left = secblitz::actions::ThreatRemoval { found: 3, removed: 1, left: 2 };
    tools_msg(&mut app, tools::Msg::ThreatsDone(Ok(left)));
    let threats = tools_status(&app, ToolsSection::Virus).expect("threats left are told");
    assert_eq!((threats.tone, threats.text.as_str()), (Tone::Warn, "Some are still there"));

    tools_msg(&mut app, tools::Msg::LookForUpdates);
    let looking = tools_status(&app, ToolsSection::Repair).expect("a running look is told");
    assert_eq!((looking.tone, looking.text.as_str()), (Tone::Neutral, "Looking for updates…"));
    tools_msg(&mut app, tools::Msg::Found(Err(("raw".into(), "network"))));
    let failed = tools_status(&app, ToolsSection::Repair).expect("a failed look is told");
    assert_eq!((failed.tone, failed.text.as_str()), (Tone::Warn, "We couldn't check for updates"));

    tools_msg(&mut app, tools::Msg::BitwardenDone(Ok(crate::broker::Reply::Done)));
    let manager = tools_status(&app, ToolsSection::Passwords).expect("an install is told");
    assert_eq!((manager.tone, manager.text.as_str()), (Tone::Good, "Bitwarden is installed"));
    tools_msg(&mut app, tools::Msg::ClearBitwarden);
    assert!(tools_status(&app, ToolsSection::Passwords).is_none());

    assert!(tools_status(&app, ToolsSection::Windows).is_none());
    app.ctx.lang = Lang::De;
    let german = tools_status(&app, ToolsSection::Virus).expect("still told");
    assert_ne!(german.text, "Some are still there", "the summary follows the language");
    app.page = Page::Tools;
    drop(app.view());
}

#[test]
fn the_summary_shows_only_while_the_section_is_closed() {
    let note = || Some(tools::Status::new(Tone::Warn, "Needs you".into()));
    assert_eq!(tools::closed_summary(false, note()), note());
    assert_eq!(tools::closed_summary(true, note()), None);
    assert_eq!(tools::closed_summary(false, None), None);
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
