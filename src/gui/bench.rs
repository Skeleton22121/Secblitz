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
