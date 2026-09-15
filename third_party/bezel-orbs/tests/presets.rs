use vitre_bezel_orbs::orbs::{OrbSize, OrbState, engine::draw_mode, resolve_preset};

#[test]
fn all_states_and_sizes_produce_finite_nonempty_frames() {
    for &state in OrbState::ALL_STATES {
        for &size in OrbSize::ALL_SIZES {
            let preset = resolve_preset(state, size);
            for time in [0., 0.6, 17.] {
                let frame = draw_mode(preset.mode, size.pixels(), time, &preset.opts);
                assert!(!frame.dots.is_empty(), "{state:?} {size:?}");
                for dot in frame.dots {
                    assert!(
                        [dot.x, dot.y, dot.z, dot.r, dot.a, dot.white]
                            .into_iter()
                            .all(f32::is_finite)
                    );
                    assert!(dot.r >= 0.);
                }
                for line in frame.lines {
                    assert!(
                        [line.x1, line.y1, line.x2, line.y2, line.w, line.a]
                            .into_iter()
                            .all(f32::is_finite)
                    );
                }
            }
        }
    }
}

#[test]
fn working_geometry_changes_with_time_and_static_frames_are_repeatable() {
    let preset = resolve_preset(OrbState::Working, OrbSize::Inline);
    let at = |t| {
        draw_mode(preset.mode, 20., t, &preset.opts)
            .dots
            .into_iter()
            .map(|d| (d.x, d.y, d.r, d.a))
            .collect::<Vec<_>>()
    };
    assert_eq!(at(0.6), at(0.6));
    assert_ne!(at(0.), at(1.));
}
