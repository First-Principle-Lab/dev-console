use dev_console_ui_tests::Profile;

#[test]
fn usage_navigation_refresh_and_return_to_commands() {
    let profile = Profile::new("usage-tabs");
    let mut stage = profile.library();
    stage.apply(|s| s.usage.data = Some(dev_console_ui_tests::usage_fixture()));
    assert!(stage.press("tab-cliproxy"));
    assert!(stage.state().usage.open);
    assert!(stage.has("usage-chart"));
    // The T3 Code sections: figures beside the chart, totals, the type share and the table.
    for key in [
        "usage-header",
        "usage-overview",
        "usage-provider-codex",
        "usage-totals",
        "usage-type-share",
        "usage-breakdown",
        "usage-model-row-0",
        "usage-model-row-1",
    ] {
        assert!(stage.has(key), "{key} is missing");
    }
    assert!(!stage.has("usage-chart-tooltip"));
    stage.apply(|s| s.usage.hover = Some(6));
    assert!(stage.has("usage-chart-tooltip"));
    stage.apply(|s| s.usage.hover = None);
    assert!(stage.press("usage-metric-1"));
    assert_eq!(stage.state().usage.metric, 1);
    assert!(stage.press("usage-window-1"));
    assert_eq!(stage.state().usage.days, 1);
    assert!(stage.press("usage-time"));
    assert!(stage.state().usage.by_time);
    assert!(stage.press("usage-metric-2"));
    assert!(stage.has("cliproxy-limits"));
    assert!(stage.has("usage-limit-session"));
    assert!(!stage.has("usage-periods"));
    assert!(!stage.has("usage-chart"));
    assert!(stage.press("usage-refresh"));
    assert!(!stage.state().usage.busy);
    assert!(stage.press("tab-commands"));
    assert!(!stage.state().usage.open);
    assert!(!stage.has("cliproxy-usage"));
}

#[test]
fn unsaved_draft_blocks_usage_navigation() {
    let profile = Profile::new("usage-unsaved");
    let mut stage = profile.library();
    stage.apply(|s| s.start_new());
    // Existing editor field behavior is covered by editor tests; use its public draft.
    stage.apply(|s| {
        if let Some(e) = &mut s.editing {
            e.name = rune::TextEdit::new("unsaved");
        }
    });
    stage.apply(|s| s.open_usage());
    assert!(!stage.state().usage.open);
    assert!(stage.state().editing.is_some());
}

#[test]
fn cli_proxy_is_a_tab_and_keeps_the_library_sidebar() {
    let profile = Profile::new("usage-sidebar");
    let id = profile.command("deploy", "echo deploy");
    let mut stage = profile.library();
    stage.apply(|s| s.usage.data = Some(dev_console_ui_tests::usage_fixture()));
    stage.press_ctrl('3');
    assert!(stage.state().usage.open);
    // The sidebar is the one Commands shows, with CLI Proxy beside the other two tabs.
    for key in [
        "tab-commands",
        "tab-secrets",
        "tab-cliproxy",
        "tab-search",
        "import-project",
        "open-environment",
        &format!("entry-row-{id}"),
    ] {
        assert!(stage.has(key), "{key} is missing beside the dashboard");
    }
    assert!(stage.has("cliproxy-usage"));
    // Choosing an entry leaves the dashboard for its details.
    assert!(stage.press(&format!("entry-row-{id}")));
    assert!(!stage.state().usage.open);
    assert!(!stage.has("cliproxy-usage"));
}

#[test]
fn a_narrow_window_shows_the_dashboard_alone_with_a_way_back() {
    let profile = Profile::new("usage-narrow");
    let mut stage = profile.library();
    stage.apply(|s| s.set_width(600.0));
    stage.apply(|s| s.usage.data = Some(dev_console_ui_tests::usage_fixture()));
    assert!(stage.press("tab-cliproxy"));
    assert!(stage.has("cliproxy-usage"));
    assert!(!stage.has("list-pane"));
    assert!(stage.press("back-to-list"));
    assert!(!stage.state().usage.open);
    assert!(stage.has("list-pane"));
}
