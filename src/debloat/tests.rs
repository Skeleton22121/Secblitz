use super::*;
use std::cell::RefCell;
use std::collections::HashSet;

const KNOWN_PROTECTED: &[&str] = &[
    "Microsoft.WindowsStore",
    "Microsoft.DesktopAppInstaller",
    "Microsoft.SecHealthUI",
    "Microsoft.VCLibs.140.00",
    "Microsoft.UI.Xaml.2.8",
    "Microsoft.NET.Native.Runtime.2.2",
    "Microsoft.WindowsTerminal",
    "Microsoft.WindowsNotepad",
    "Microsoft.WindowsCalculator",
    "Microsoft.ScreenSketch",
    "Microsoft.XboxIdentityProvider",
    "Microsoft.MicrosoftEdge.Stable",
    "Microsoft.OneDriveSync",
    "Microsoft.HEIFImageExtension",
    "Microsoft.VP9VideoExtensions",
    "Microsoft.WebMediaExtensions",
    "Microsoft.WebpImageExtension",
    "Microsoft.AV1VideoExtension",
    "Microsoft.HEVCVideoExtension",
    "Microsoft.RawImageExtension",
    "Microsoft.Windows.ShellExperienceHost",
    "Microsoft.StorePurchaseApp",
    "Microsoft.Services.Store.Engagement",
];

#[test]
fn indices_are_unique_and_names_are_unique() {
    let mut families = HashSet::new();
    let mut names = HashSet::new();
    for app in catalog() {
        assert!(
            families.insert(app.family.to_ascii_lowercase()),
            "{}",
            app.family
        );
        assert!(names.insert(app.name), "{}", app.name);
    }
    assert!(catalog().len() < u16::MAX as usize);
}

#[test]
fn protected_packages_are_protected_and_never_in_the_catalog() {
    for name in KNOWN_PROTECTED {
        assert!(is_protected(name), "{name} must be protected");
        assert_eq!(catalog::owner(name), None, "{name} must not be removable");
        assert!(
            !catalog().iter().any(|a| pattern_matches(a.family, name)),
            "{name} matched by catalog"
        );
    }
    for app in catalog() {
        let stem = app.family.trim_end_matches('*');
        assert!(
            !is_protected(stem),
            "catalog entry {} is protected",
            app.family
        );
        assert!(catalog::is_valid_package_name(stem), "{}", app.family);
        if let Some(rest) = app.family.strip_suffix('*') {
            assert!(
                !rest.is_empty() && rest.ends_with(['.', '-']),
                "{}",
                app.family
            );
        }
    }
}

#[test]
fn defaults_come_only_from_recommended_and_sponsored() {
    for g in Group::ALL {
        let expected = matches!(g, Group::Recommended | Group::Sponsored);
        assert_eq!(g.selected_by_default(), expected);
    }
    assert!(catalog().iter().any(|a| a.group == Group::Recommended));
    assert!(catalog().iter().any(|a| a.group == Group::Gaming));
}

#[test]
fn store_ids_look_like_store_ids() {
    for app in catalog() {
        if let Some(id) = app.store_id {
            // Packaged apps have 12-character ids (9N...), Store-listed
            // desktop apps 14 (XP...).
            assert!(
                id.len() == 12 || (id.len() == 14 && id.starts_with("XP")),
                "{}",
                app.name
            );
            assert!(id
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()));
        }
    }
    let ids: Vec<_> = catalog().iter().filter_map(|a| a.store_id).collect();
    assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
}

#[test]
fn prefix_patterns_match_only_their_publisher() {
    assert!(pattern_matches("king.com.*", "king.com.CandyCrushSaga"));
    assert!(pattern_matches("king.com.*", "King.com.CandyCrushSodaSaga"));
    assert!(!pattern_matches("king.com.*", "kingdom.thing"));
    assert!(pattern_matches("Microsoft.BingNews", "microsoft.bingnews"));
    assert!(!pattern_matches(
        "Microsoft.BingNews",
        "Microsoft.BingNewsPlus"
    ));
}

#[test]
fn invalid_names_never_match() {
    for bad in [
        "",
        "king.com.x; calc",
        "Microsoft.BingNews\"",
        "a b",
        "Microsoft.BingNews\n",
    ] {
        assert_eq!(catalog::owner(bad), None, "{bad:?}");
    }
}

#[test]
fn inventory_json_is_filtered_strictly_to_the_catalog() {
    let json = r#"[
      {"name":"Microsoft.BingNews","version":"1.0","installed":true,"provisioned":true,"nonRemovable":false,"framework":false},
      {"name":"Microsoft.WindowsStore","version":"2","installed":true,"provisioned":false,"nonRemovable":false,"framework":false},
      {"name":"Microsoft.Xbox.TCUI","version":"1","installed":true,"provisioned":false,"nonRemovable":true,"framework":false},
      {"name":"king.com.CandyCrushSaga","version":"3","installed":true,"provisioned":false,"nonRemovable":false,"framework":false},
      {"name":"Contoso.Unknown","version":"1","installed":true,"provisioned":false,"nonRemovable":false,"framework":false},
      {"name":"Microsoft.UI.Xaml.2.8","version":"8","installed":true,"provisioned":false,"nonRemovable":false,"framework":true}
    ]"#;
    let found = parse_inventory(json).unwrap();
    let names: Vec<_> = found.iter().map(|p| p.package.as_str()).collect();
    assert_eq!(names, ["Microsoft.BingNews", "king.com.CandyCrushSaga"]);
    // A lone object (PowerShell unwraps single items) is accepted too.
    let one =
        r#"{"name":"Microsoft.BingNews","version":"1.0","nonRemovable":false,"framework":false}"#;
    assert_eq!(parse_inventory(one).unwrap().len(), 1);
    assert_eq!(parse_inventory("[]").unwrap().len(), 0);
    assert!(parse_inventory("garbage").is_err());
}

#[test]
fn outcome_parsing() {
    assert_eq!(
        parse_outcome(r#"{"removed":true,"protected":false,"error":null}"#),
        PackageOutcome::Removed
    );
    assert_eq!(
        parse_outcome(r#"{"removed":false,"protected":true}"#),
        PackageOutcome::Protected
    );
    assert!(
        matches!(parse_outcome(r#"{"removed":false,"protected":false,"error":"x"}"#), PackageOutcome::Failed(e) if e == "x")
    );
    assert!(matches!(parse_outcome("nope"), PackageOutcome::Failed(_)));
}

fn idx(family: &str) -> u16 {
    catalog().iter().position(|a| a.family == family).unwrap() as u16
}

#[test]
fn request_for_unknown_index_is_refused() {
    let out = remove_with(
        &[u16::MAX],
        &[],
        &|_| Ok(()),
        &|_| PackageOutcome::Removed,
        &|_| {},
    );
    assert!(out.is_err());
}

#[test]
fn removal_never_runs_for_protected_or_foreign_packages() {
    let news = idx("Microsoft.BingNews");
    let installed = vec![
        Installed {
            index: news,
            package: "Microsoft.BingNews".into(),
            version: "1".into(),
        },
        // A forged inventory entry pointing a catalog index at a protected package.
        Installed {
            index: news,
            package: "Microsoft.WindowsStore".into(),
            version: "1".into(),
        },
        Installed {
            index: news,
            package: "Evil; calc".into(),
            version: "1".into(),
        },
    ];
    let seen = RefCell::new(Vec::new());
    let batch = remove_with(
        &[news],
        &installed,
        &|_| Ok(()),
        &|p| {
            seen.borrow_mut().push(p.to_string());
            PackageOutcome::Removed
        },
        &|_| {},
    )
    .unwrap();
    assert_eq!(*seen.borrow(), ["Microsoft.BingNews"]);
    assert_eq!(batch.removed.len(), 1);
    assert_eq!(batch.removed_apps(), 1);
}

#[test]
fn removal_reports_removed_protected_and_failed() {
    let news = idx("Microsoft.BingNews");
    let weather = idx("Microsoft.BingWeather");
    let maps = idx("Microsoft.WindowsMaps");
    let installed = vec![
        Installed {
            index: news,
            package: "Microsoft.BingNews".into(),
            version: "1".into(),
        },
        Installed {
            index: weather,
            package: "Microsoft.BingWeather".into(),
            version: "2".into(),
        },
        Installed {
            index: maps,
            package: "Microsoft.WindowsMaps".into(),
            version: "3".into(),
        },
    ];
    let events = RefCell::new(Vec::new());
    let batch = remove_with(
        &[news, weather, maps, news],
        &installed,
        &|_| Ok(()),
        &|p| match p {
            "Microsoft.BingNews" => PackageOutcome::Removed,
            "Microsoft.BingWeather" => PackageOutcome::Protected,
            _ => PackageOutcome::Failed("boom".into()),
        },
        &|e| events.borrow_mut().push(e),
    )
    .unwrap();
    assert_eq!(batch.removed.len(), 1);
    assert_eq!(batch.skipped, [weather]);
    assert_eq!(
        batch.failed,
        [Failure {
            index: maps,
            reason: "boom".into()
        }]
    );
    let events = events.into_inner();
    assert_eq!(events.len(), 9, "each app saves, starts and finishes once");
    assert_eq!(events[0], Progress::Saving(news));
    assert_eq!(events[1], Progress::Started(news));
    assert_eq!(events[2], Progress::Finished(news, ItemResult::Removed));
    assert_eq!(
        events[5],
        Progress::Finished(weather, ItemResult::Protected)
    );
}

#[test]
fn no_copy_means_no_removal() {
    let index = catalog::owner("Microsoft.BingWeather").unwrap();
    let installed = vec![Installed {
        index,
        package: "Microsoft.BingWeather".into(),
        version: "1".into(),
    }];
    let ran = std::cell::Cell::new(false);
    let batch = remove_with(
        &[index],
        &installed,
        &|_| Err(Kept::NoSpace),
        &|_| {
            ran.set(true);
            PackageOutcome::Removed
        },
        &|_| {},
    )
    .unwrap();
    assert!(!ran.get(), "never removed without a copy");
    assert!(batch.removed.is_empty() && batch.failed.is_empty());
    assert_eq!(batch.kept, vec![index]);
}

#[test]
fn apps_that_are_not_installed_are_ignored() {
    let news = idx("Microsoft.BingNews");
    let batch = remove_with(
        &[news],
        &[],
        &|_| panic!("must not save"),
        &|_| panic!("must not run"),
        &|_| {},
    )
    .unwrap();
    assert_eq!(
        batch,
        Batch {
            t: batch.t,
            ..Batch::default()
        }
    );
}

#[test]
fn journal_round_trip_and_restore_marking() {
    let dir = std::env::temp_dir().join(format!("secblitz-debloat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(journal::FILE);
    let _ = std::fs::remove_file(&path);
    assert!(journal::load_from(&path).is_empty());
    let batch = Batch {
        t: 42,
        removed: vec![Removed {
            index: 3,
            package: "Microsoft.GetHelp".into(),
            version: "1".into(),
            restored: false,
        }],
        skipped: vec![5],
        failed: vec![Failure {
            index: 7,
            reason: "x".into(),
        }],
        kept: vec![],
    };
    journal::append_to(&path, &batch).unwrap();
    journal::append_to(&path, &batch).unwrap();
    assert_eq!(
        journal::load_from(&path),
        vec![batch.clone(), batch.clone()]
    );
    journal::mark_restored_in(&path, 3).unwrap();
    assert!(journal::load_from(&path)
        .iter()
        .all(|b| b.removed[0].restored));
    // Malformed lines are skipped.
    std::fs::write(&path, "not json\n").unwrap();
    assert!(journal::load_from(&path).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn offline_scripts_are_plain_ascii_and_emit_json() {
    for (name, text) in [
        ("describe", include_str!("scripts/describe.ps1")),
        ("register", include_str!("scripts/register.ps1")),
    ] {
        assert!(text.is_ascii(), "{name}.ps1 must be ASCII");
        assert!(!text.contains('\r'), "{name}.ps1 must use LF endings");
        assert!(text.contains("ConvertTo-Json"), "{name}.ps1 prints JSON");
    }
}

#[test]
fn still_removed_lists_each_app_once_and_skips_restored_ones() {
    let item = |index, restored| Removed {
        index,
        package: "p".into(),
        version: "1".into(),
        restored,
    };
    let batch = |t, removed| Batch {
        t,
        removed,
        skipped: vec![],
        failed: vec![],
        kept: vec![],
    };
    let journal = [
        batch(10, vec![item(3, true), item(4, true), item(5, false)]),
        batch(
            20,
            vec![item(3, false), item(5, false), item(u16::MAX, false)],
        ),
    ];
    // Removed again after a restore counts once, at its newest time; restored
    // apps and indices outside the catalog are left out.
    assert_eq!(
        journal::still_removed(&journal, catalog().len()),
        vec![(3, 20), (5, 20)]
    );
    assert!(journal::still_removed(&[], catalog().len()).is_empty());
}

#[test]
fn restore_all_sorts_outcomes() {
    use super::offline::Restored::*;
    assert_eq!(classify(Some(Ok(Back)), false), Bucket::Restored);
    assert_eq!(
        classify(Some(Ok(BackWithoutSomeData)), true),
        Bucket::Restored
    );
    assert_eq!(classify(Some(Ok(AlreadyThere)), false), Bucket::Restored);
    assert_eq!(classify(Some(Ok(Damaged)), true), Bucket::NeedsStore);
    assert_eq!(classify(Some(Ok(NoCopy)), false), Bucket::Failed);
    assert_eq!(
        classify(Some(Err(anyhow::anyhow!("x"))), true),
        Bucket::NeedsStore
    );
    assert_eq!(
        classify(Some(Err(anyhow::anyhow!("x"))), false),
        Bucket::Failed
    );
    assert_eq!(classify(None, true), Bucket::NeedsStore);
    assert_eq!(classify(None, false), Bucket::Failed);
}
