//! Measured throughput: real time coordinates, honest gaps, presentation-only scrolling.
use super::*;

type Sample = (SystemTime, Option<u64>, Option<u64>);
const WINDOW: Duration = Duration::from_secs(60);
const MAX_GAP: Duration = Duration::from_secs(2);

fn age(time: SystemTime, end: SystemTime) -> f32 {
    match end.duration_since(time) {
        Ok(value) => value.as_secs_f32(),
        Err(value) => -value.duration().as_secs_f32(),
    }
}

fn visible(history: &RateHistory) -> Vec<Sample> {
    let Some((end, _, _)) = history.last() else {
        return Vec::new();
    };
    history
        .iter()
        .copied()
        .filter(|(time, _, _)| {
            end.duration_since(*time)
                .is_ok_and(|elapsed| elapsed <= WINDOW)
        })
        .collect()
}

pub(super) fn peaks(history: &RateHistory) -> (Option<u64>, Option<u64>) {
    let samples = visible(history);
    (
        samples.iter().filter_map(|s| s.1).max(),
        samples.iter().filter_map(|s| s.2).max(),
    )
}

fn runs(samples: &[Sample], upload: bool) -> Vec<Vec<(SystemTime, u64)>> {
    let mut result = Vec::new();
    let mut run = Vec::new();
    let mut previous = None;
    for &(time, down, up) in samples {
        let gap = previous.is_some_and(|old| {
            time.duration_since(old)
                .map_or(true, |elapsed| elapsed.is_zero() || elapsed > MAX_GAP)
        });
        let value = if upload { up } else { down };
        if (gap || value.is_none()) && !run.is_empty() {
            result.push(std::mem::take(&mut run));
        }
        if let Some(value) = value {
            run.push((time, value));
        }
        previous = Some(time);
    }
    if !run.is_empty() {
        result.push(run);
    }
    result
}

#[derive(Clone)]
struct Scroll {
    from: SystemTime,
    to: SystemTime,
    started: f64,
}

impl Scroll {
    fn end(&self, now: f64, duration: f64) -> SystemTime {
        let fraction = if duration <= 0.0 {
            1.0
        } else {
            ((now - self.started) / duration).clamp(0.0, 1.0)
        };
        self.from
            + self
                .to
                .duration_since(self.from)
                .unwrap_or_default()
                .mul_f64(fraction)
    }
}

pub(super) fn paint(ui: &mut Ui, history: &RateHistory, language: Language) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, RATE_CHART_HEIGHT + 20.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let samples = visible(history);
    let peak = samples.iter().flat_map(|s| [s.1, s.2]).flatten().max();
    let axis = rate_axis(peak);
    let plot = egui::Rect::from_min_max(
        rect.min + egui::vec2(52.0, 22.0),
        rect.max - egui::vec2(8.0, 25.0),
    );
    if plot.width() < 1.0 {
        return;
    }
    let y = |value: f32| plot.bottom() - value / axis.max_bytes * plot.height();
    // The plot area gets a hairline box so the series read as a chart rather than as floating
    // lines on the card, matching the bordered blocks elsewhere on the page.
    painter.rect_stroke(plot, 0.0, Stroke::new(1.0_f32, scale::border()));
    for i in 0..=axis.step_count {
        let ordinate = y(axis.step_bytes() * i as f32);
        painter.line_segment(
            [
                egui::pos2(plot.left(), ordinate),
                egui::pos2(plot.right(), ordinate),
            ],
            Stroke::new(1.0_f32, scale::line()),
        );
        painter.text(
            egui::pos2(plot.left() - 10.0, ordinate),
            egui::Align2::RIGHT_CENTER,
            axis.tick_label(i),
            egui::FontId::proportional(scale::META),
            scale::faint(),
        );
    }
    painter.text(
        egui::pos2(plot.left(), rect.top()),
        egui::Align2::LEFT_TOP,
        axis.unit,
        egui::FontId::proportional(scale::META),
        scale::faint(),
    );
    for seconds in [60, 45, 30, 15, 0] {
        if width < 420.0 && matches!(seconds, 45 | 15) {
            continue;
        }
        let text = if seconds == 0 {
            LocalizedText::new(language, TextKey::RateNow).text
        } else {
            format_text_in(
                language,
                TextKey::RateSecondsAgo,
                &TextArgs::count(seconds as usize),
            )
            .text
        };
        let align = match seconds {
            60 => egui::Align2::LEFT_CENTER,
            0 => egui::Align2::RIGHT_CENTER,
            _ => egui::Align2::CENTER_CENTER,
        };
        painter.text(
            egui::pos2(
                plot.right() - seconds as f32 / RATE_CHART_X_SPAN_SECS * plot.width(),
                plot.bottom() + 16.0,
            ),
            align,
            text,
            egui::FontId::proportional(scale::META),
            scale::faint(),
        );
    }
    let Some(&(latest, _, _)) = samples.last() else {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            LocalizedText::new(language, TextKey::RateSampling).text,
            egui::FontId::proportional(scale::RATE_AUX),
            scale::secondary(),
        );
        return;
    };
    if SystemTime::now()
        .duration_since(latest)
        .is_ok_and(|elapsed| elapsed > Duration::from_secs(3))
    {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            LocalizedText::new(language, TextKey::RateSamplePaused).text,
            egui::FontId::proportional(scale::RATE_AUX),
            scale::secondary(),
        );
        return;
    }
    if peak.is_none() {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            LocalizedText::new(language, TextKey::RateNotSampled).text,
            egui::FontId::proportional(scale::RATE_AUX),
            scale::secondary(),
        );
        return;
    }
    let now = ui.input(|input| input.time);
    let duration = if ui.style().animation_time > 0.0 {
        0.42
    } else {
        0.0
    };
    let id = response.id.with("measured-rate-scroll");
    let end = ui.ctx().data_mut(|data| {
        let mut state = data.get_temp::<Scroll>(id).unwrap_or(Scroll {
            from: latest,
            to: latest,
            started: now,
        });
        if state.to != latest {
            let from = state.end(now, duration);
            state = if latest.duration_since(state.to).is_ok_and(|d| d <= MAX_GAP) {
                Scroll {
                    from,
                    to: latest,
                    started: now,
                }
            } else {
                Scroll {
                    from: latest,
                    to: latest,
                    started: now,
                }
            };
        }
        let end = state.end(now, duration);
        data.insert_temp(id, state);
        end
    });
    if end != latest {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
    let x = |time| plot.right() - age(time, end) / RATE_CHART_X_SPAN_SECS * plot.width();
    let series = painter.with_clip_rect(plot.expand(1.0));
    for (upload, color) in [(false, DOWN_COLOR), (true, UP_COLOR)] {
        for run in runs(&samples, upload) {
            let points: Vec<_> = run
                .iter()
                .map(|&(time, value)| egui::pos2(x(time), y(value as f32)))
                .collect();
            if !upload {
                // Separate quads preserve gaps and avoid triangulating a non-convex area.
                for pair in points.windows(2) {
                    let mut mesh = egui::Mesh::default();
                    for point in [pair[0], pair[1]] {
                        mesh.colored_vertex(point, color.linear_multiply(0.08));
                    }
                    for point in [pair[1], pair[0]] {
                        mesh.colored_vertex(
                            egui::pos2(point.x, plot.bottom()),
                            Color32::TRANSPARENT,
                        );
                    }
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    series.add(Shape::mesh(mesh));
                }
            }
            if points.len() > 1 {
                if upload {
                    series.add(Shape::dashed_line(
                        &points,
                        Stroke::new(1.8_f32, color),
                        5.0_f32,
                        4.0_f32,
                    ));
                } else {
                    series.add(Shape::line(points.clone(), Stroke::new(2.3_f32, color)));
                }
            } else if let Some(&point) = points.first() {
                series.circle_filled(point, 2.5, color);
            }
            if run.last().is_some_and(|p| p.0 == latest) {
                if let Some(&point) = points.last() {
                    series.circle_filled(point, 3.5, color);
                }
            }
        }
    }
    if let Some(pointer) = response.hover_pos().filter(|point| plot.contains(*point)) {
        // Snap to an actual stored sample. Never interpolate an unmeasured tooltip value.
        let nearest = samples
            .iter()
            .filter(|s| x(s.0) <= plot.right())
            .min_by(|a, b| {
                (x(a.0) - pointer.x)
                    .abs()
                    .total_cmp(&(x(b.0) - pointer.x).abs())
            });
        if let Some(&(time, down, up)) = nearest {
            let hx = x(time);
            // Do not show a remote sample as if it were inside a sampling interruption.
            if (hx - pointer.x).abs() <= plot.width() * 1.5 / RATE_CHART_X_SPAN_SECS {
                series.line_segment(
                    [egui::pos2(hx, plot.top()), egui::pos2(hx, plot.bottom())],
                    Stroke::new(1.0_f32, scale::line()),
                );
                for (value, color) in [(down, DOWN_COLOR), (up, UP_COLOR)] {
                    if let Some(value) = value {
                        series.circle_filled(egui::pos2(hx, y(value as f32)), 4.0, color);
                    }
                }
                response.on_hover_ui_at_pointer(|ui| {
                    ui.label(
                        format_text_in(
                            language,
                            TextKey::RateHoverAgo,
                            &TextArgs::age(format!("{:.1}", age(time, latest))),
                        )
                        .text,
                    );
                    let missing = || LocalizedText::new(language, TextKey::ValueNotAvailable).text;
                    ui.label(
                        format_text_in(
                            language,
                            TextKey::RateHoverDown,
                            &TextArgs::detail(down.map(format_rate_1dp).unwrap_or_else(missing)),
                        )
                        .text,
                    );
                    ui.label(
                        format_text_in(
                            language,
                            TextKey::RateHoverUp,
                            &TextArgs::detail(up.map(format_rate_1dp).unwrap_or_else(missing)),
                        )
                        .text,
                    );
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }
    #[test]
    fn production_default_history_stays_bounded() {
        let mut history = RateHistory::default();
        for i in 0..600 {
            history.push((at(i), Some(i), Some(0)));
        }
        assert_eq!(history.len(), RATE_HISTORY_CAPACITY);
        assert_eq!(history.last().unwrap().1, Some(599));
    }
    #[test]
    fn real_time_gaps_and_series_missing_values_are_independent() {
        let samples = [
            (at(0), Some(1), Some(2)),
            (at(1), None, Some(3)),
            (at(8), Some(4), Some(5)),
        ];
        assert_eq!(age(at(1), at(8)), 7.0);
        assert_eq!(
            runs(&samples, false)
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![1, 1]
        );
        assert_eq!(
            runs(&samples, true)
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
    }
    #[test]
    fn expired_peaks_are_excluded_and_zero_is_a_measurement() {
        let mut history = RateHistory::new();
        history.push((at(0), Some(9000), Some(8000)));
        history.push((at(61), Some(0), None));
        assert_eq!(peaks(&history), (Some(0), None));
        assert_eq!(visible(&history).len(), 1);
        assert_eq!(runs(&visible(&history), false)[0][0].1, 0);
    }
    #[test]
    fn reversed_or_duplicate_timestamps_do_not_connect() {
        let samples = [
            (at(2), Some(1), None),
            (at(2), Some(2), None),
            (at(1), Some(3), None),
        ];
        assert_eq!(runs(&samples, false).len(), 3);
    }
    #[test]
    fn scroll_only_changes_time_coordinates_and_settles_exactly() {
        let scroll = Scroll {
            from: at(10),
            to: at(11),
            started: 0.0,
        };
        assert_eq!(scroll.end(0.21, 0.42), at(10) + Duration::from_millis(500));
        assert_eq!(scroll.end(1.0, 0.42), at(11));
        assert_eq!(scroll.end(0.0, 0.0), at(11));
    }
}
