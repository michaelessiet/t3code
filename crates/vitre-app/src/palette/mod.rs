//! The palette surfaces: quick-open / content search (`quick_search`) and the
//! command palette (`command_palette`), sharing the ranking rules in [`rank`].

pub mod command_palette;
pub mod preview;
pub mod quick_search;
pub mod rank;

/// Electron's `formatRelativeTimeLabel` (`apps/web/src/timestampFormat.ts`),
/// which spells out "ago" unlike the sidebar's compact form. Both palettes
/// stamp their rows with it.
pub fn relative_time(iso: &str) -> Option<String> {
    let then = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    let delta = chrono::Utc::now().signed_duration_since(then.with_timezone(&chrono::Utc));
    let minutes = delta.num_minutes();
    Some(if minutes < 1 {
        "just now".into()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if delta.num_hours() < 24 {
        format!("{}h ago", delta.num_hours())
    } else {
        format!("{}d ago", delta.num_days())
    })
}
