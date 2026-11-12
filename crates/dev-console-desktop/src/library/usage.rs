//! CLI Proxy usage, with the same Cost / Tokens / Limits hierarchy as T3 Code.
use super::Library;
use crate::{
    icons,
    theme::{size, text as ts, Look},
    widgets,
};
use dev_console_core::cliproxy::{CliproxyUsageResponse, UsageRecord};
use rune::{col, el, row, text, El, ScrollSpec};
use std::collections::{BTreeMap, BTreeSet};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Default)]
pub struct Usage {
    pub open: bool,
    pub metric: usize,
    pub days: usize,
    pub by_time: bool,
    pub data: Option<CliproxyUsageResponse>,
    pub busy: bool,
    pub error: Option<String>,
    /// The chart period under the pointer, and the plot width that maps the pointer onto it.
    pub hover: Option<usize>,
    plot_w: f32,
    serial: u64,
}
impl Library {
    pub fn open_usage(&mut self) {
        if self.draft_dirty() {
            self.status = Some(super::BUSY_EDIT.into());
            return;
        }
        self.cancel_edit();
        self.selected_id = None;
        self.details = None;
        self.selected_suggestion = None;
        self.usage.open = true;
        if self.usage.days == 0 {
            self.usage.days = 7;
        }
        if self.usage.data.is_none() {
            self.refresh_usage();
        }
    }
    pub fn refresh_usage(&mut self) {
        self.usage.serial += 1;
        let serial = self.usage.serial;
        self.usage.busy = true;
        self.usage.error = None;
        let since = (OffsetDateTime::now_utc() - time::Duration::days(90))
            .format(&Rfc3339)
            .ok();
        self.exec().clone().run(
            self,
            move || dev_console_core::cliproxy::cliproxy_usage(since, None),
            move |state, result| {
                if state.usage.serial != serial {
                    return;
                }
                state.usage.busy = false;
                match result {
                    Ok(data) => state.usage.data = Some(data),
                    Err(e) => state.usage.error = Some(e.to_string()),
                }
            },
        );
    }
}
#[derive(Clone, Copy)]
struct Priced {
    /// Input, cache read, cache write and output, in dollars.
    parts: [f64; 4],
    /// What the cached input would have cost uncached.
    savings: f64,
}
// Estimates from the same explicit local rate table as the web companion. Unknown models
// stay unpriced; this is API-equivalent value, never a subscription invoice.
fn cost(record: &UsageRecord) -> Option<Priced> {
    let (input, cache, output, tier) = match record.model.as_str() {
        "gpt-6.1-sol" => (2.0, 0.1, 10.0, true),
        "gpt-6-sol" => (2.0, 0.2, 10.0, false),
        "gpt-6-astra" => (10.0, 1.0, 50.0, true),
        "gpt-6.1-terra" => (1.0, 0.05, 5.0, true),
        "gpt-6-luna" => (0.1, 0.01, 0.5, true),
        "gpt-5.6-sol" => (4.0, 0.4, 20.0, false),
        "gpt-5.6-luna" => (0.2, 0.02, 1.2, false),
        "gpt-5.3-codex" | "gpt-5.3" => (1.75, 0.175, 14.0, false),
        _ => return None,
    };
    let long = tier
        && record.input_tokens + record.cached_input_tokens + record.cache_creation_tokens
            > 272_000;
    let (k_in, k_out) = if long { (2.0, 1.5) } else { (1.0, 1.0) };
    let m = |tokens: u64, rate: f64, k: f64| tokens as f64 * rate * k / 1e6;
    Some(Priced {
        parts: [
            m(record.input_tokens, input, k_in),
            m(record.cached_input_tokens, cache, k_in),
            m(record.cache_creation_tokens, input * 1.25, k_in),
            m(record.output_tokens, output, k_out),
        ],
        savings: m(record.cached_input_tokens, input - cache, k_in),
    })
}
/// Content at least this wide puts the figures beside the chart, as T3 Code's `lg` layout does.
const WIDE: f32 = 760.0;
const PERIODS: [(usize, &str); 4] = [
    (1, "Past 24h"),
    (7, "7 days"),
    (30, "30 days"),
    (90, "90 days"),
];
const TYPES: [&str; 4] = ["Input", "Cache read", "Cache write", "Output"];
// T3 Code mixes the foreground into the background at these strengths for the type segments.
const TYPE_INK: [f32; 4] = [0.6, 0.3, 0.72, 1.0];

fn label(value: impl Into<String>, look: &Look) -> El {
    text(value).font(ts::SM).color(look.muted)
}
fn caption(value: impl Into<String>, look: &Look) -> El {
    text(value).font(ts::XS).color(look.muted)
}
fn heading(value: impl Into<String>, look: &Look) -> El {
    text(value).font(ts::MEDIUM).color(look.fg)
}
fn usd(value: f64) -> String {
    let cents = (value * 100.0).round() as i64;
    let whole = (cents / 100).to_string();
    let mut grouped = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("${grouped}.{:02}", cents % 100)
}
/// `formatTokens` from T3 Code: two significant decimals under 10, one under 100, none above.
fn compact(value: f64) -> String {
    for (scale, suffix) in [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")] {
        if value >= scale {
            let v = value / scale;
            let digits = if v >= 100.0 {
                0
            } else if v >= 10.0 {
                1
            } else {
                2
            };
            let s = format!("{v:.digits$}");
            let s = if s.contains('.') {
                s.trim_end_matches('0').trim_end_matches('.').to_owned()
            } else {
                s
            };
            return format!("{s}{suffix}");
        }
    }
    format!("{value:.0}")
}
fn amount(value: f64, metric: usize) -> String {
    if metric == 0 {
        usd(value)
    } else {
        compact(value)
    }
}
fn percent(share: f64) -> String {
    let p = share * 100.0;
    if p > 0.0 && p < 0.1 {
        "<0.1%".into()
    } else {
        format!("{p:.1}%")
    }
}
fn day_short(t: OffsetDateTime) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{} {}", MONTHS[t.month() as usize - 1], t.day())
}
fn period_label(t: OffsetDateTime, hourly: bool) -> String {
    if hourly {
        format!("{:02}:00", t.hour())
    } else {
        day_short(t)
    }
}
fn tokens(r: &UsageRecord) -> f64 {
    (r.input_tokens + r.cached_input_tokens + r.cache_creation_tokens + r.output_tokens) as f64
}
/// A T3 Code segmented control: a sunken track with the selected option raised.
fn segmented(look: &Look, key: &str, items: Vec<El>) -> El {
    let mut group = row()
        .key(key.to_owned())
        .role("tablist")
        .h(size::CONTROL_H_SM + 4.0)
        .padding(2.0)
        .gap(2.0)
        .center_y()
        .radius(size::RADIUS + 1.0)
        .fill(look.sunken)
        .border(1.0, look.line_hair);
    for item in items {
        group = group.child(item);
    }
    group
}
fn segment(look: &Look, key: String, title: &str, selected: bool) -> El {
    row()
        .key(key)
        .role("tab")
        .label(title.to_owned())
        .checked(selected)
        .focusable(true)
        .focus_ring(2.0, 1.0, look.focus)
        .h(size::CONTROL_H_SM)
        .px(10.0)
        .center()
        .radius(size::RADIUS)
        .fill(if selected {
            look.raised
        } else {
            rune::Color::TRANSPARENT
        })
        .border(
            1.0,
            if selected {
                look.line
            } else {
                rune::Color::TRANSPARENT
            },
        )
        .hover(look.fg.alpha(0.06))
        .cursor(rune::Cursor::Pointer)
        .child(text(title.to_owned()).font(ts::SM).color(if selected {
            look.fg
        } else {
            look.muted
        }))
}
pub fn view(state: &Library, look: &Look) -> El {
    let u = &state.usage;
    let sidebar = if state.narrow {
        0.0
    } else {
        state.sidebar_width()
    };
    let wide = state.window_width - sidebar >= WIDE;
    let mut controls = row().gap(8.0).center_y();
    if u.metric != 2 {
        // Limits are always current, so they have no period; it sits first so dropping it
        // never moves the metric control.
        controls = controls.child(segmented(
            look,
            "usage-periods",
            PERIODS
                .iter()
                .map(|&(days, name)| {
                    segment(look, format!("usage-window-{days}"), name, u.days == days)
                        .on_press(move |s: &mut Library| s.usage.days = days)
                })
                .collect(),
        ));
    }
    controls = controls.child(segmented(
        look,
        "usage-metrics",
        ["Cost", "Tokens", "Limits"]
            .iter()
            .enumerate()
            .map(|(i, name)| {
                segment(look, format!("usage-metric-{i}"), name, u.metric == i)
                    .on_press(move |s: &mut Library| s.usage.metric = i)
            })
            .collect(),
    ));
    let refresh = widgets::icon_button(
        look,
        "usage-refresh",
        if u.metric == 2 {
            "Refresh limits"
        } else {
            "Refresh usage"
        },
        &icons::UPDATE,
        look.muted,
    );
    controls = controls.child(if u.busy {
        widgets::disabled(refresh)
    } else {
        refresh.on_press(|s: &mut Library| s.refresh_usage())
    });
    let mut lead = row().gap(6.0).center_y();
    if state.narrow {
        // The sidebar is hidden in a narrow window, so the dashboard carries the way back.
        lead = lead.child(
            widgets::button_sm(
                look,
                "back-to-list",
                "Library",
                Some(&icons::BACK),
                widgets::Variant::Ghost,
            )
            .on_press_tx(|s: &mut Library, tx| {
                tx.instant();
                s.back_to_list();
            }),
        );
    }
    let head = row()
        .key("usage-header")
        .w_full()
        .min_h(48.0)
        .py(8.0)
        .pl(20.0)
        .pr(8.0)
        .gap(12.0)
        .center_y()
        .flex_wrap()
        .border_sides([0.0, 0.0, 1.0, 0.0], look.line_hair)
        .child(
            lead.child(text("CLI Proxy").font(ts::SM).color(look.muted))
                .child(text("/").font(ts::SM).color(look.dim))
                .child(text("Usage").font(ts::MEDIUM).color(look.fg)),
        )
        .child(el().grow(1.0))
        .child(controls);
    let mut body = col()
        .key("cliproxy-usage")
        .w_full()
        .max_w(1100.0)
        .px(20.0)
        .py(24.0)
        .gap(24.0);
    if let Some(error) = &u.error {
        body = body.child(text(error.clone()).color(look.danger).wrap().w_full());
    }
    if u.busy && u.data.is_none() {
        body = body.child(label("Loading usage…", look));
    }
    body = if u.metric == 2 {
        body.child(limits(u, look))
    } else {
        overview(u, look, wide, body)
    };
    col().w_full().grow(1.0).min_h(0.0).child(head).child(
        col()
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(body),
    )
}
fn limits(u: &Usage, look: &Look) -> El {
    let mut body = col().key("cliproxy-limits").w_full().gap(12.0);
    let Some(data) = &u.data else {
        return body.child(label("Loading Codex limits…", look));
    };
    let plan = data.limits.as_ref().and_then(|l| l.plan.as_ref());
    body = body.child(
        row()
            .gap(8.0)
            .center_y()
            .child(el().w(8.0).h(8.0).radius(4.0).fill(look.fg))
            .child(heading("Codex", look))
            .child(caption(plan.cloned().unwrap_or_default(), look)),
    );
    if let Some(error) = &data.limits_error {
        return body.child(label(error, look).wrap().w_full());
    }
    let Some(limits) = &data.limits else {
        return body.child(label("Codex reports no subscription limits.", look));
    };
    if limits.windows.is_empty() {
        body = body.child(label("Codex reports no limit windows.", look));
    }
    let now = OffsetDateTime::now_utc();
    for window in &limits.windows {
        let remaining = (100.0 - window.used_percent).clamp(0.0, 100.0);
        let left = window
            .resets_at
            .as_deref()
            .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
            .map(|r| (r - now).whole_minutes().max(0));
        // Where even spending would be: the share of the window still to run.
        let time_left = match (left, window.window_duration_mins) {
            (Some(left), Some(d)) if d > 0 => Some((left as f64 / d as f64 * 100.0).min(100.0)),
            _ => None,
        };
        let pace = time_left.map(|t| {
            if remaining + 2.0 < t {
                "Ahead of pace"
            } else if remaining > t + 2.0 {
                "Under pace"
            } else {
                "On pace"
            }
        });
        let resets = left.map(|m| {
            if m >= 1440 {
                format!("↻ {}d {}h", m / 1440, m % 1440 / 60)
            } else if m >= 60 {
                format!("↻ {}h {}m", m / 60, m % 60)
            } else {
                format!("↻ {m}m")
            }
        });
        let mut bar = row()
            .key(format!("usage-limit-{}", window.id))
            .role("img")
            .label(format!(
                "{}: {remaining:.0}% left{}{}",
                window.label,
                time_left
                    .map(|t| format!(", {t:.0}% of the window left"))
                    .unwrap_or_default(),
                resets
                    .as_deref()
                    .map(|r| format!(", resets in {}", r.trim_start_matches("↻ ")))
                    .unwrap_or_default()
            ))
            .grow(1.0)
            .min_w(0.0)
            .h(32.0)
            .radius(size::RADIUS)
            .fill(look.fg.alpha(0.07))
            .child(
                el().pin(Some(0.0), Some(0.0), None, Some(0.0))
                    .w_frac((remaining / 100.0) as f32)
                    .radius(size::RADIUS)
                    .fill(if remaining < 10.0 {
                        look.danger.alpha(0.5)
                    } else {
                        look.fg.alpha(0.35)
                    }),
            );
        if let Some(t) = time_left {
            bar = bar.child(
                row()
                    .pin(Some(0.0), Some(2.0), Some(0.0), Some(2.0))
                    .child(el().w_frac((t / 100.0) as f32))
                    .child(el().w(2.0).h_full().radius(1.0).fill(look.fg)),
            );
        }
        if let Some(resets) = resets {
            bar = bar.child(
                row()
                    .pin(None, Some(0.0), Some(8.0), Some(0.0))
                    .center_y()
                    .child(
                        row()
                            .px(6.0)
                            .py(2.0)
                            .radius(size::RADIUS_SM)
                            .fill(look.panel.alpha(0.85))
                            .child(text(resets).font(ts::XS).color(look.fg)),
                    ),
            );
        }
        let mut summary = row()
            .gap(8.0)
            .items_end()
            .child(
                text(format!("{remaining:.0}%"))
                    .font(rune::tok::TextStyle {
                        size: 30.0,
                        ..ts::DETAIL_TITLE
                    })
                    .color(look.fg),
            )
            .child(label("left", look).pb(4.0));
        if let Some(pace) = pace {
            summary = summary.child(caption(pace, look).pb(5.0));
        }
        body = body.child(
            row()
                .w_full()
                .padding(16.0)
                .gap(24.0)
                .center_y()
                .flex_wrap()
                .radius(size::RADIUS_LG - 2.0)
                .border(1.0, look.line_hair)
                .child(
                    col()
                        .w(176.0)
                        .gap(4.0)
                        .child(heading(window.label.clone(), look))
                        .child(summary),
                )
                .child(bar),
        );
    }
    body.child(caption(
        format!(
            "Checked {}",
            OffsetDateTime::parse(&limits.checked_at, &Rfc3339)
                .map(|t| format!("{} {:02}:{:02} UTC", day_short(t), t.hour(), t.minute()))
                .unwrap_or_else(|_| limits.checked_at.clone())
        ),
        look,
    ))
}
/// `niceScale` from T3 Code: a 1/2/5 step that gives at most `count` intervals.
fn nice_scale(peak: f64, count: usize) -> (f64, Vec<f64>) {
    if peak <= 0.0 {
        return (count as f64, (0..=count).map(|i| i as f64).collect());
    }
    let raw = peak / count as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let n = raw / magnitude;
    let step = magnitude
        * if n > 5.0 {
            10.0
        } else if n > 2.0 {
            5.0
        } else if n > 1.0 {
            2.0
        } else {
            1.0
        };
    let max = (peak / step).ceil() * step;
    let ticks = (0..=(max / step).round() as usize)
        .map(|i| i as f64 * step)
        .collect();
    (max, ticks)
}
/// T3 Code's monotone cubic (Fritsch-Carlson tangents) as Bézier segments, sampled densely
/// enough to read as a curve; it never overshoots a point, so a quiet day never dips below zero.
fn smooth(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let n = points.len();
    if n < 3 {
        return points.to_vec();
    }
    let slope = |i: usize| {
        (points[i + 1][1] - points[i][1]) / (points[i + 1][0] - points[i][0]).max(f32::EPSILON)
    };
    let d: Vec<f32> = (0..n - 1).map(slope).collect();
    let mut m = vec![0.0f32; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for i in 1..n - 1 {
        m[i] = if d[i - 1] * d[i] <= 0.0 {
            0.0
        } else {
            (d[i - 1] + d[i]) / 2.0
        };
    }
    for i in 0..n - 1 {
        if d[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[i] / d[i], m[i + 1] / d[i]);
        let h = a.hypot(b);
        if h > 3.0 {
            m[i] = 3.0 / h * a * d[i];
            m[i + 1] = 3.0 / h * b * d[i];
        }
    }
    let mut out = Vec::new();
    for i in 0..n - 1 {
        let (p0, p3) = (points[i], points[i + 1]);
        let dx = p3[0] - p0[0];
        let p1 = [p0[0] + dx / 3.0, p0[1] + m[i] * dx / 3.0];
        let p2 = [p3[0] - dx / 3.0, p3[1] - m[i + 1] * dx / 3.0];
        for s in 0..12 {
            let t = s as f32 / 12.0;
            let u = 1.0 - t;
            let at = |k: usize| {
                u * u * u * p0[k]
                    + 3.0 * u * u * t * p1[k]
                    + 3.0 * u * t * t * p2[k]
                    + t * t * t * p3[k]
            };
            out.push([at(0), at(1)]);
        }
    }
    out.push(points[n - 1]);
    out
}
fn chart(buckets: &[f64], periods: &[OffsetDateTime], u: &Usage, hourly: bool, look: &Look) -> El {
    const W: f32 = 960.0;
    const H: f32 = 260.0;
    const TOP: f32 = 8.0;
    const PLOT: f32 = 224.0;
    let peak = buckets.iter().copied().fold(0.0, f64::max);
    let (max, ticks) = nice_scale(peak, 4);
    let y = |v: f64| H - (v / max) as f32 * (H - TOP);
    let step = W / (buckets.len().max(2) - 1) as f32;
    let points: Vec<[f32; 2]> = buckets
        .iter()
        .enumerate()
        .map(|(i, v)| [i as f32 * step, y(*v)])
        .collect();
    let line = smooth(&points);
    let mut area = vec![[0.0, H]];
    area.extend(line.iter().copied());
    area.push([W, H]);
    let mut shape = rune::Shape::new().view_box(W, H);
    for tick in &ticks {
        let ty = y(*tick);
        shape = shape.stroke(vec![[0.0, ty], [W, ty]], 1.0, look.line_hair);
    }
    shape = shape
        .fill(area, look.fg.alpha(0.12))
        .stroke(line, 2.0, look.fg);
    let hover = u.hover.filter(|i| *i < buckets.len());
    if let Some(i) = hover {
        let x = i as f32 * step;
        shape = shape.stroke(vec![[x, TOP], [x, H]], 1.0, look.muted);
    }
    let mut axis = col().w(56.0).h(PLOT);
    for tick in &ticks {
        let top = y(*tick) / H * PLOT - 7.0;
        axis = axis.child(
            text(if *tick == 0.0 {
                "0".into()
            } else {
                amount(*tick, u.metric)
            })
            .font(TINY)
            .color(look.muted)
            .pin(None, Some(top), Some(0.0), None),
        );
    }
    let n = buckets.len();
    let mut plot = row()
        .grow(1.0)
        .min_w(0.0)
        .h(PLOT)
        .on_layout(|s: &mut Library, rect: [f32; 4]| s.usage.plot_w = rect[2])
        .on_pointer_move_local(move |s: &mut Library, p, _| {
            let w = s.usage.plot_w.max(1.0);
            let i = (p.x / w * (n.max(2) - 1) as f32)
                .round()
                .clamp(0.0, (n - 1) as f32);
            s.usage.hover = Some(i as usize);
        })
        .on_hover_change(|s: &mut Library, inside, _| {
            if !inside {
                s.usage.hover = None;
            }
        });
    plot = plot.child(
        rune::canvas(shape)
            .key("usage-chart")
            .label(format!(
                "{} {} by provider",
                if hourly { "Hourly" } else { "Daily" },
                if u.metric == 0 {
                    "cost"
                } else {
                    "processed tokens"
                }
            ))
            .w_full()
            .h_full(),
    );
    if let (Some(i), Some(t)) = (hover, periods.get(hover.unwrap_or(0))) {
        // T3 Code's tooltip: the period, then each provider's value beside its dot.
        let x = i as f32 / (n.max(2) - 1) as f32 * u.plot_w;
        let left = if x > u.plot_w - 170.0 {
            x - 160.0
        } else {
            x + 10.0
        };
        plot = plot.child(
            col()
                .key("usage-chart-tooltip")
                .pin(Some(left.max(0.0)), Some(8.0), None, None)
                .min_w(144.0)
                .px(10.0)
                .py(8.0)
                .gap(4.0)
                .radius(size::RADIUS_LG + 2.0)
                .fill(look.raised)
                .border(1.0, look.line)
                .child(caption(period_label(*t, hourly), look))
                .child(
                    row()
                        .gap(8.0)
                        .center_y()
                        .child(el().w(8.0).h(8.0).radius(4.0).fill(look.fg))
                        .child(text("Codex").font(ts::XS).color(look.fg))
                        .child(el().grow(1.0))
                        .child(
                            text(amount(buckets[i], u.metric))
                                .font(ts::XS)
                                .color(look.fg),
                        ),
                ),
        );
    }
    let edge = |i: usize| {
        periods
            .get(i)
            .map(|t| period_label(*t, hourly).to_uppercase())
            .unwrap_or_default()
    };
    col()
        .w_full()
        .gap(4.0)
        .child(row().w_full().gap(8.0).child(axis).child(plot))
        .child(
            row()
                .w_full()
                .pl(64.0)
                .child(text(edge(0)).font(TINY).color(look.muted))
                .child(el().grow(1.0))
                .child(text(edge(periods.len() / 2)).font(TINY).color(look.muted))
                .child(el().grow(1.0))
                .child(text(edge(periods.len() - 1)).font(TINY).color(look.muted)),
        )
}
const TINY: rune::tok::TextStyle = rune::tok::TextStyle::new(10.0, 400.0);
fn share_bar(title: &str, parts: [f64; 4], metric: usize, look: &Look) -> El {
    let total: f64 = parts.iter().sum();
    let mut bar = row()
        .key("usage-type-share")
        .role("img")
        .label(format!(
            "{title}: {}",
            TYPES
                .iter()
                .zip(parts)
                .filter(|(_, v)| *v > 0.0)
                .map(|(name, v)| format!("{name} {}", amount(v, metric)))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .w_full()
        .h(8.0)
        .gap(2.0);
    let mut legend = row().w_full().gap(16.0).flex_wrap();
    let visible: Vec<_> = (0..4).filter(|&i| parts[i] > 0.0).collect();
    for (n, &i) in visible.iter().enumerate() {
        let color = look.fg.alpha(TYPE_INK[i]);
        let r = size::RADIUS_SM / 2.0;
        let end = 4.0;
        bar = bar.child(
            el().w(0.0)
                .min_w(4.0)
                .grow((parts[i] / total) as f32)
                .h_full()
                .corners([
                    if n == 0 { end } else { r },
                    if n + 1 == visible.len() { end } else { r },
                    if n + 1 == visible.len() { end } else { r },
                    if n == 0 { end } else { r },
                ])
                .fill(color),
        );
        legend = legend.child(
            row()
                .gap(6.0)
                .center_y()
                .child(el().w(8.0).h(8.0).radius(2.0).fill(color))
                .child(caption(TYPES[i], look))
                .child(text(amount(parts[i], metric)).font(ts::XS).color(look.fg)),
        );
    }
    col()
        .w_full()
        .gap(10.0)
        .child(heading(title, look))
        .child(bar)
        .child(legend)
}
/// Right-aligned table cell of a fixed width.
fn cell(content: El, width: f32) -> El {
    row().w(width).justify_end().child(content)
}
#[derive(Default)]
struct Model {
    cost: f64,
    priced: bool,
    tokens: f64,
}
fn overview(u: &Usage, look: &Look, wide: bool, mut body: El) -> El {
    let now = OffsetDateTime::now_utc();
    let days = u.days.max(1);
    let hourly = days == 1;
    let start = if hourly {
        (now - time::Duration::hours(23))
            .replace_time(time::Time::from_hms(now.hour(), 0, 0).unwrap())
    } else {
        (now - time::Duration::days(days as i64 - 1)).replace_time(time::Time::MIDNIGHT)
    };
    let unit = if hourly { 3600 } else { 86400 };
    let count = if hourly { 24 } else { days };
    let periods: Vec<_> = (0..count)
        .map(|i| start + time::Duration::seconds(i as i64 * unit))
        .collect();
    let records: Vec<_> = u
        .data
        .iter()
        .flat_map(|d| &d.records)
        .filter_map(|r| {
            OffsetDateTime::parse(&r.ts, &Rfc3339)
                .ok()
                .filter(|t| *t >= start && *t <= now)
                .map(|t| (t, r))
        })
        .collect();
    let metric_of = |cost: f64, tokens: f64| if u.metric == 0 { cost } else { tokens };
    let mut sessions = BTreeSet::new();
    let mut models: BTreeMap<String, Model> = BTreeMap::new();
    let mut buckets = vec![(0.0f64, 0.0f64); count];
    let mut types = [0.0; 4];
    let mut type_cost = [0.0; 4];
    let mut savings = 0.0;
    let mut unpriced = false;
    for (t, r) in &records {
        sessions.insert(&r.session);
        let priced = cost(r);
        unpriced |= priced.is_none();
        let parts = priced.map(|p| p.parts).unwrap_or_default();
        let spent: f64 = parts.iter().sum();
        let model = models
            .entry(if r.model.is_empty() {
                "Unknown model".into()
            } else {
                r.model.clone()
            })
            .or_default();
        model.cost += spent;
        model.priced |= priced.is_some();
        model.tokens += tokens(r);
        let index = ((*t - start).whole_seconds() / unit) as usize;
        if let Some(bucket) = buckets.get_mut(index) {
            bucket.0 += spent;
            bucket.1 += tokens(r);
        }
        for (i, v) in [
            r.input_tokens,
            r.cached_input_tokens,
            r.cache_creation_tokens,
            r.output_tokens,
        ]
        .into_iter()
        .enumerate()
        {
            types[i] += v as f64;
            type_cost[i] += parts[i];
        }
        savings += priced.map(|p| p.savings).unwrap_or_default();
    }
    let total_cost: f64 = type_cost.iter().sum();
    let total_tokens: f64 = types.iter().sum();
    let total = metric_of(total_cost, total_tokens);
    let series: Vec<f64> = buckets.iter().map(|(c, t)| metric_of(*c, *t)).collect();

    let mut sub = row().gap(4.0).center_y().child(caption(
        format!(
            "{} {}{}",
            sessions.len(),
            if sessions.len() == 1 {
                "session"
            } else {
                "sessions"
            },
            if u.metric == 0 {
                " · API estimate"
            } else {
                ""
            }
        ),
        look,
    ));
    if u.metric == 0 && unpriced {
        sub = sub.child(
            text("ⓘ")
                .key("usage-unpriced")
                .font(ts::XS)
                .color(look.muted)
                .help("API estimate excludes unpriced models."),
        );
    }
    let share_of = |v: f64| if total > 0.0 { v / total } else { 0.0 };
    let figures = col().gap(20.0);
    let figures = if wide {
        figures.w(288.0)
    } else {
        figures.w_full()
    };
    let figures = figures
        .child(
            col()
                .gap(4.0)
                .child(
                    text(amount(total, u.metric))
                        .key("usage-total")
                        .font(rune::tok::TextStyle {
                            size: 36.0,
                            ..ts::DETAIL_TITLE
                        })
                        .color(look.fg),
                )
                .child(sub),
        )
        .child(
            col()
                .key("usage-provider-codex")
                .w_full()
                .gap(4.0)
                .child(
                    row()
                        .w_full()
                        .gap(8.0)
                        .center_y()
                        .child(el().w(8.0).h(8.0).radius(4.0).fill(look.fg))
                        .child(text("Codex").font(ts::BODY).color(look.fg))
                        .child(
                            text(format!("{} sessions", sessions.len()))
                                .font(TINY)
                                .color(look.muted),
                        )
                        .child(el().grow(1.0))
                        .child(
                            text(amount(total, u.metric))
                                .font(ts::MEDIUM)
                                .color(look.fg),
                        ),
                )
                .child(caption(
                    if u.metric == 0 {
                        format!(
                            "{} of cost · {} tokens",
                            percent(share_of(total)),
                            compact(total_tokens)
                        )
                    } else {
                        format!(
                            "{} of tokens · {}",
                            percent(share_of(total)),
                            usd(total_cost)
                        )
                    },
                    look,
                )),
        );
    let graph = col()
        .min_w(0.0)
        .w_full()
        .gap(12.0)
        .child(heading(
            format!(
                "{} {}",
                if hourly { "Hourly" } else { "Daily" },
                if u.metric == 0 {
                    "cost"
                } else {
                    "processed tokens"
                }
            ),
            look,
        ))
        .child(chart(&series, &periods, u, hourly, look));
    body = body.child(if wide {
        row()
            .key("usage-overview")
            .w_full()
            .gap(24.0)
            .child(figures)
            .child(graph.grow(1.0))
    } else {
        col()
            .key("usage-overview")
            .w_full()
            .gap(24.0)
            .child(figures)
            .child(graph)
    });

    let mut totals = row().key("usage-totals").w_full().gap(24.0).flex_wrap();
    for (title, value) in [
        ("Processed tokens", compact(total_tokens)),
        ("Cached input", compact(types[1])),
        ("Uncached input", compact(types[0])),
        ("Output", compact(types[3])),
        ("Cache savings", usd(savings)),
    ] {
        totals = totals.child(
            col()
                .grow(1.0)
                .min_w(110.0)
                .gap(2.0)
                .child(caption(title, look))
                .child(text(value).font(ts::LG).color(look.fg)),
        );
    }
    body = body.child(
        col()
            .w_full()
            .gap(8.0)
            .child(heading("Totals", look))
            .child(totals.py(4.0)),
    );
    if total_tokens > 0.0 {
        body = body.child(if u.metric == 0 {
            share_bar("Cost by type", type_cost, 0, look)
        } else {
            share_bar("Tokens by type", types, 1, look)
        });
    }

    let header = |cols: Vec<(&str, f32)>| {
        let mut r = row()
            .w_full()
            .py(8.0)
            .px(8.0)
            .gap(0.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], look.line);
        for (i, (name, width)) in cols.into_iter().enumerate() {
            let t = caption(name, look);
            r = r.child(if width == 0.0 {
                row().grow(1.0).min_w(0.0).child(t)
            } else if i == 0 {
                row().w(width).child(t)
            } else {
                cell(t, width)
            });
        }
        r
    };
    let mut table = col().key("usage-breakdown").w_full();
    let empty = |look: &Look| {
        row()
            .w_full()
            .py(24.0)
            .justify_center()
            .child(label("No activity in this window.", look))
    };
    if u.by_time {
        let first = if hourly { "Hour" } else { "Day" };
        table = table.child(header(vec![
            (first, 0.0),
            ("Codex", 96.0),
            ("Total", 96.0),
            ("Tokens", 96.0),
        ]));
        let active: Vec<_> = periods
            .iter()
            .zip(&buckets)
            .filter(|(_, b)| b.1 > 0.0)
            .rev()
            .collect();
        if active.is_empty() {
            table = table.child(empty(look));
        }
        for (t, (c, k)) in active {
            table = table.child(
                row()
                    .w_full()
                    .py(8.0)
                    .px(8.0)
                    .border_sides([0.0, 0.0, 1.0, 0.0], look.line_hair)
                    .hover(look.fg.alpha(0.04))
                    .child(
                        row()
                            .grow(1.0)
                            .child(text(period_label(*t, hourly)).font(ts::BODY).color(look.fg)),
                    )
                    .child(cell(label(usd(*c), look), 96.0))
                    .child(cell(text(usd(*c)).font(ts::BODY).color(look.fg), 96.0))
                    .child(cell(label(compact(*k), look), 96.0)),
            );
        }
    } else {
        table = table.child(header(vec![
            ("#", 28.0),
            ("Model", 0.0),
            ("Cost", 96.0),
            ("Share", 72.0),
            ("Tokens", 88.0),
        ]));
        let mut ranked: Vec<_> = models.into_iter().collect();
        ranked.sort_by(|a, b| {
            metric_of(b.1.cost, b.1.tokens).total_cmp(&metric_of(a.1.cost, a.1.tokens))
        });
        let peak = ranked
            .first()
            .map(|(_, m)| metric_of(m.cost, m.tokens))
            .unwrap_or(0.0);
        if ranked.is_empty() {
            table = table.child(empty(look));
        }
        for (index, (name, m)) in ranked.iter().enumerate() {
            let value = metric_of(m.cost, m.tokens);
            let mut model = col()
                .grow(1.0)
                .min_w(0.0)
                .gap(6.0)
                .child(text(name.clone()).font(ts::BODY).color(look.fg).truncate());
            if value > 0.0 && peak > 0.0 {
                model = model.child(
                    row().w_full().max_w(192.0).h(2.0).child(
                        el().w_frac((value / peak) as f32)
                            .min_w(8.0)
                            .h_full()
                            .radius(1.0)
                            .fill(look.fg),
                    ),
                );
            }
            let share = metric_of(
                if total_cost > 0.0 {
                    m.cost / total_cost
                } else {
                    0.0
                },
                if total_tokens > 0.0 {
                    m.tokens / total_tokens
                } else {
                    0.0
                },
            );
            table = table.child(
                row()
                    .key(format!("usage-model-row-{index}"))
                    .w_full()
                    .py(10.0)
                    .px(8.0)
                    .center_y()
                    .border_sides([0.0, 0.0, 1.0, 0.0], look.line_hair)
                    .hover(look.fg.alpha(0.04))
                    .child(row().w(28.0).child(caption((index + 1).to_string(), look)))
                    .child(model)
                    .child(cell(
                        if m.priced {
                            text(usd(m.cost)).font(ts::BODY).color(look.fg)
                        } else {
                            label("Unpriced", look)
                        },
                        96.0,
                    ))
                    .child(cell(
                        label(
                            if m.priced || u.metric == 1 {
                                percent(share)
                            } else {
                                String::new()
                            },
                            look,
                        ),
                        72.0,
                    ))
                    .child(cell(label(compact(m.tokens), look), 88.0)),
            );
        }
    }
    body.child(
        col()
            .w_full()
            .gap(12.0)
            .child(
                row()
                    .w_full()
                    .gap(12.0)
                    .center_y()
                    .child(heading("Breakdown", look))
                    .child(el().grow(1.0))
                    .child(segmented(
                        look,
                        "usage-breakdown-toggle",
                        vec![
                            segment(look, "usage-model".into(), "Model", !u.by_time)
                                .on_press(|s: &mut Library| s.usage.by_time = false),
                            segment(
                                look,
                                "usage-time".into(),
                                if hourly { "Hour" } else { "Day" },
                                u.by_time,
                            )
                            .on_press(|s: &mut Library| s.usage.by_time = true),
                        ],
                    )),
            )
            .child(table),
    )
    .child(
        caption(
            "Cost and tokens come from local Codex transcripts; limits come from the proxy account.",
            look,
        )
        .wrap()
        .w_full(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn figures_use_the_t3_code_formats() {
        assert_eq!(usd(1234.5), "$1,234.50");
        assert_eq!(usd(0.004), "$0.00");
        assert_eq!(compact(1_430_000.0), "1.43M");
        assert_eq!(compact(770_000.0), "770K");
        assert_eq!(compact(12_340.0), "12.3K");
        assert_eq!(compact(2_000_000.0), "2M");
        assert_eq!(compact(999.0), "999");
        assert_eq!(percent(0.0004), "<0.1%");
        assert_eq!(percent(0.971), "97.1%");
    }

    #[test]
    fn the_axis_steps_by_one_two_or_five() {
        assert_eq!(
            nice_scale(0.37, 4),
            (0.4, vec![0.0, 0.1, 0.2, 0.30000000000000004, 0.4])
        );
        let (max, ticks) = nice_scale(218_000.0, 4);
        assert_eq!(max, 300_000.0);
        assert_eq!(ticks.len(), 4);
        assert_eq!(nice_scale(0.0, 4).1.len(), 5);
    }

    #[test]
    fn the_curve_never_overshoots_its_points() {
        let points = [[0.0, 10.0], [10.0, 250.0], [20.0, 12.0], [30.0, 250.0]];
        let line = smooth(&points);
        assert!(line.iter().all(|p| (10.0..=250.0).contains(&p[1])));
        assert_eq!(line.last(), points.last());
    }

    #[test]
    fn cost_splits_by_type_and_counts_cache_savings() {
        let record = UsageRecord {
            ts: String::new(),
            session: String::new(),
            model: "gpt-6-sol".into(),
            input_tokens: 1_000_000,
            cached_input_tokens: 1_000_000,
            cache_creation_tokens: 0,
            output_tokens: 1_000_000,
            reasoning_tokens: 0,
        };
        let priced = cost(&record).expect("a known model is priced");
        assert_eq!(priced.parts, [2.0, 0.2, 0.0, 10.0]);
        assert!((priced.savings - 1.8).abs() < 1e-9);
        assert!(cost(&UsageRecord {
            model: "unknown".into(),
            ..record
        })
        .is_none());
    }
}
