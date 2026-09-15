use std::{
    ops::Range,
    sync::{Arc, Mutex},
};

use gpui::{
    AbsoluteLength, AnyElement, App, AvailableSpace, Bounds, DefiniteLength, Element, ElementId,
    GlobalElementId, HighlightStyle, InspectorElementId, InteractiveElement as _, IntoElement,
    LayoutId, LineFragment as WrapLineFragment, ObjectFit, ParentElement as _, Pixels, ShapedLine,
    SharedString, SharedUri, Size, StatefulInteractiveElement as _, Styled, StyledImage as _,
    TextRun, TextStyle, WhiteSpace, Window, div, img, point, prelude::FluentBuilder as _, px,
    relative, size,
};

use crate::{
    ActiveTheme as _,
    text::text_view::{LinkClickHandlerFn, handle_link_click},
    tooltip::Tooltip,
};

use super::{
    inline::{Inline, InlineState},
    node::{LinkMark, normalize_inline_code_text},
    utils::image_source,
};

const IMAGE_LEN: usize = 1;

/// Split at semantic code marks so padding is layout, never extra characters
/// in the selectable/copied text. Links and emphasis keep their byte ranges.
pub(super) fn text_items(
    text: SharedString,
    state: Arc<Mutex<InlineState>>,
    links: Vec<(Range<usize>, LinkMark)>,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    mut codes: Vec<Range<usize>>,
) -> Vec<InlineFlowItem> {
    codes.sort_by_key(|range| range.start);
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for range in codes {
        if range.is_empty() || range.end > text.len() {
            continue;
        }
        if let Some(last) = ranges.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            ranges.push(range);
        }
    }
    if ranges.is_empty() {
        return vec![InlineFlowItem::Text {
            state,
            text,
            links,
            highlights,
        }];
    }
    let mut chunks = Vec::new();
    let mut offset = 0;
    for range in ranges {
        if offset < range.start {
            chunks.push((offset..range.start, false));
        }
        offset = range.end;
        chunks.push((range, true));
    }
    if offset < text.len() {
        chunks.push((offset..text.len(), false));
    }
    chunks
        .into_iter()
        .map(|(range, code)| {
            let text: SharedString = if code {
                normalize_inline_code_text(&text[range.clone()]).into()
            } else {
                text[range.clone()].to_owned().into()
            };
            let inline_state = InlineState::fragment(text.clone(), state.clone(), range.start);
            let state = Arc::new(Mutex::new(inline_state));
            let links = slice_ranges(&links, range.start, range.end, |range, link| {
                (range, link.clone())
            });
            let highlights = slice_ranges(&highlights, range.start, range.end, |range, style| {
                (range, *style)
            });
            if code {
                InlineFlowItem::Code {
                    text,
                    state,
                    links,
                    highlights,
                }
            } else {
                InlineFlowItem::Text {
                    text,
                    state,
                    links,
                    highlights,
                }
            }
        })
        .collect()
}

fn measure_code(
    text: &SharedString,
    style: &TextStyle,
    wrap_width: Option<Pixels>,
    window: &mut Window,
) -> Size<Pixels> {
    let font_size = style.font_size.to_pixels(window.rem_size());
    let runs = [style.to_run(text.len())];
    // Taffy rounds inner widths to device pixels. Leave a pixel of shaping
    // slack so a fitting word doesn't wrap its final glyph after rounding.
    let natural =
        px(f32::from(shape_line(text.clone(), font_size, &runs, window).width()).ceil() + 12.);
    let width = natural.min(wrap_width.unwrap_or(natural)).max(px(12.));
    let lines = window
        .text_system()
        .shape_text(text.clone(), font_size, &runs, Some(width - px(10.)), None)
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.wrap_boundaries.len() + 1)
                .sum::<usize>()
        })
        .unwrap_or(1);
    size(
        width,
        style.line_height_in_pixels(window.rem_size()) * lines.max(1) as f32 + px(4.),
    )
}

pub(super) struct InlineFlow {
    id: ElementId,
    items: Vec<InlineFlowItem>,
    link_click_handler: Option<Arc<LinkClickHandlerFn>>,
}

pub(super) enum InlineFlowItem {
    Code {
        state: Arc<Mutex<InlineState>>,
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    },
    Text {
        state: Arc<Mutex<InlineState>>,
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    },
    Image {
        url: SharedUri,
        link: Option<LinkMark>,
        title: String,
        width: Option<DefiniteLength>,
        height: Option<DefiniteLength>,
    },
}

#[derive(Default)]
pub(crate) struct InlineFlowLayoutState {
    layout: Arc<Mutex<Option<InlineFlowLayout>>>,
}

#[derive(Default)]
struct InlineFlowLayout {
    fragments: Vec<PositionedFragment>,
    size: Size<Pixels>,
}

#[derive(Clone)]
enum PositionedFragment {
    Code {
        item_ix: usize,
        origin: gpui::Point<Pixels>,
        size: Size<Pixels>,
    },
    Text {
        item_ix: usize,
        origin: gpui::Point<Pixels>,
        size: Size<Pixels>,
        source_range: Range<usize>,
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    },
    Image {
        item_ix: usize,
        origin: gpui::Point<Pixels>,
        size: Size<Pixels>,
    },
}

enum MeasureItem {
    Code {
        text: SharedString,
        style: TextStyle,
    },
    Text {
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    },
    Image {
        url: SharedUri,
        width: Option<DefiniteLength>,
        height: Option<DefiniteLength>,
    },
}

struct LineFragmentLayout {
    item_ix: usize,
    kind: LineFragmentKind,
    size: Size<Pixels>,
    source_range: Range<usize>,
}

enum LineFragmentKind {
    Code,
    Text {
        text: SharedString,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    },
    Image,
}

impl InlineFlow {
    pub(super) fn new(
        id: impl Into<ElementId>,
        items: Vec<InlineFlowItem>,
        link_click_handler: Option<Arc<LinkClickHandlerFn>>,
    ) -> Self {
        Self {
            id: id.into(),
            items,
            link_click_handler,
        }
    }

    fn image_element(
        ix: usize,
        url: &SharedUri,
        link: &Option<LinkMark>,
        title: &str,
        size: Size<Pixels>,
        link_click_handler: Option<Arc<LinkClickHandlerFn>>,
    ) -> AnyElement {
        img(image_source(url))
            .id(ix)
            .object_fit(ObjectFit::Contain)
            .max_w(relative(1.))
            .w(size.width)
            .h(size.height)
            .when_some(link.clone(), |this, link| {
                let title = title.to_string();
                let aux_link = link.clone();
                let aux_link_click_handler = link_click_handler.clone();
                this.cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(title.clone()).build(window, cx))
                    .on_click(move |event, window, cx| {
                        gpui_base::TextSelection::end(window, cx);
                        cx.stop_propagation();
                        handle_link_click(
                            &link_click_handler,
                            link.url.clone(),
                            event.clone(),
                            window,
                            cx,
                        );
                    })
                    .on_aux_click(move |event, window, cx| {
                        gpui_base::TextSelection::end(window, cx);
                        cx.stop_propagation();
                        handle_link_click(
                            &aux_link_click_handler,
                            aux_link.url.clone(),
                            event.clone(),
                            window,
                            cx,
                        );
                    })
            })
            .into_any_element()
    }
}

impl IntoElement for InlineFlow {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for InlineFlow {
    type RequestLayoutState = InlineFlowLayoutState;
    type PrepaintState = Vec<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let measure_items = self
            .items
            .iter()
            .map(|item| match item {
                InlineFlowItem::Code { text, .. } => {
                    let mut style = window.text_style();
                    style.font_family = cx.theme().mono_font_family.clone();
                    MeasureItem::Code {
                        text: text.clone(),
                        style,
                    }
                }
                _ => MeasureItem::from(item),
            })
            .collect::<Vec<_>>();
        let line_height = window.line_height();
        let rem_size = window.rem_size();
        let image_sizes = measure_items
            .iter()
            .enumerate()
            .map(|(ix, item)| match item {
                MeasureItem::Image { url, width, height } => Some(measure_image_size(
                    ix,
                    url,
                    *width,
                    *height,
                    line_height,
                    rem_size,
                    window,
                    cx,
                )),
                MeasureItem::Text { .. } | MeasureItem::Code { .. } => None,
            })
            .collect::<Vec<_>>();
        let layout_state = InlineFlowLayoutState::default();
        let layout_ref = layout_state.layout.clone();
        // The measured-layout callback runs outside the paragraph's inherited
        // text-style stack. Capture it here or plain fragments get wider gaps
        // when a chat uses a smaller font than the window default.
        let text_style = window.text_style();

        let layout_id = window.request_measured_layout(Default::default(), {
            move |known_dimensions, available_space, window, _cx| {
                let wrap_width = if text_style.white_space == WhiteSpace::Normal {
                    known_dimensions.width.or(match available_space.width {
                        AvailableSpace::Definite(width) => Some(width),
                        _ => None,
                    })
                } else {
                    None
                };
                let layout = layout_flow(
                    &measure_items,
                    &image_sizes,
                    &text_style,
                    wrap_width,
                    window,
                );
                let size = layout.size;
                if let Ok(mut state) = layout_ref.lock() {
                    *state = Some(layout);
                }
                size
            }
        });

        (layout_id, layout_state)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let fragments = request_layout
            .layout
            .lock()
            .ok()
            .and_then(|layout| layout.as_ref().map(|layout| layout.fragments.clone()))
            .unwrap_or_default();
        let mut elements = Vec::with_capacity(fragments.len());

        for item in &self.items {
            if let InlineFlowItem::Text { state, .. } | InlineFlowItem::Code { state, .. } = item
                && let Ok(mut state) = state.lock()
            {
                state.clear_fragment_selection();
            }
        }

        for fragment in fragments {
            match fragment {
                PositionedFragment::Code {
                    item_ix,
                    origin,
                    size: fragment_size,
                } => {
                    let InlineFlowItem::Code {
                        state,
                        text,
                        links,
                        highlights,
                    } = &self.items[item_ix]
                    else {
                        continue;
                    };
                    if let Ok(mut state) = state.lock() {
                        state.set_text(text.clone());
                    }
                    let mut highlights = highlights.clone();
                    for (_, style) in &mut highlights {
                        style.background_color = None;
                    }
                    let mut element = div()
                        .id(("inline-code", item_ix))
                        .debug_selector(|| format!("inline-code-{item_ix}"))
                        .w(fragment_size.width)
                        .h(fragment_size.height)
                        .px(px(4.))
                        .py(px(1.))
                        .border_1()
                        .rounded(px(4.))
                        .border_color(cx.theme().foreground.opacity(0.12))
                        .bg(cx.theme().foreground.opacity(0.06))
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(
                            div()
                                .w_full()
                                .debug_selector(move || format!("inline-code-content-{item_ix}"))
                                .child(Inline::new(
                                    item_ix,
                                    state.clone(),
                                    links.clone(),
                                    highlights,
                                    self.link_click_handler.clone(),
                                )),
                        )
                        .into_any_element();
                    element.prepaint_as_root(
                        bounds.origin + origin,
                        size(
                            AvailableSpace::Definite(fragment_size.width),
                            AvailableSpace::Definite(fragment_size.height),
                        ),
                        window,
                        cx,
                    );
                    elements.push(element);
                }
                PositionedFragment::Text {
                    item_ix,
                    origin,
                    size: fragment_size,
                    source_range,
                    text,
                    links,
                    highlights,
                    ..
                } => {
                    let state = match &self.items[item_ix] {
                        InlineFlowItem::Text {
                            state,
                            text: source,
                            ..
                        } if source_range == (0..source.len()) => state.clone(),
                        InlineFlowItem::Text { state, .. } => Arc::new(Mutex::new(
                            InlineState::fragment(text.clone(), state.clone(), source_range.start),
                        )),
                        _ => Arc::new(Mutex::new(InlineState::default())),
                    };
                    if let Ok(mut state) = state.lock() {
                        state.set_text(text);
                    }

                    let mut element = Inline::new(
                        elements.len(),
                        state,
                        links,
                        highlights,
                        self.link_click_handler.clone(),
                    )
                    .into_any_element();
                    element.prepaint_as_root(
                        bounds.origin + origin,
                        size(
                            AvailableSpace::Definite(fragment_size.width),
                            AvailableSpace::Definite(fragment_size.height),
                        ),
                        window,
                        cx,
                    );
                    elements.push(element);
                }
                PositionedFragment::Image {
                    item_ix,
                    origin,
                    size: fragment_size,
                } => {
                    let InlineFlowItem::Image {
                        url, link, title, ..
                    } = &self.items[item_ix]
                    else {
                        continue;
                    };
                    let mut element = Self::image_element(
                        elements.len(),
                        url,
                        link,
                        title.as_str(),
                        fragment_size,
                        self.link_click_handler.clone(),
                    );
                    element.prepaint_as_root(
                        bounds.origin + origin,
                        size(
                            AvailableSpace::Definite(fragment_size.width),
                            AvailableSpace::Definite(fragment_size.height),
                        ),
                        window,
                        cx,
                    );
                    elements.push(element);
                }
            }
        }

        elements
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for element in prepaint {
            element.paint(window, cx);
        }
    }
}

impl From<&InlineFlowItem> for MeasureItem {
    fn from(item: &InlineFlowItem) -> Self {
        match item {
            InlineFlowItem::Code { .. } => unreachable!("code measurement needs the current font"),
            InlineFlowItem::Text {
                state: _,
                text,
                links,
                highlights,
                ..
            } => MeasureItem::Text {
                text: text.clone(),
                links: links.clone(),
                highlights: highlights.clone(),
            },
            InlineFlowItem::Image {
                url, width, height, ..
            } => MeasureItem::Image {
                url: url.clone(),
                width: *width,
                height: *height,
            },
        }
    }
}

impl MeasureItem {
    fn len(&self) -> usize {
        match self {
            MeasureItem::Text { text, .. } => text.len(),
            MeasureItem::Image { .. } | MeasureItem::Code { .. } => IMAGE_LEN,
        }
    }
}

fn layout_flow(
    items: &[MeasureItem],
    image_sizes: &[Option<Size<Pixels>>],
    text_style: &TextStyle,
    wrap_width: Option<Pixels>,
    window: &mut Window,
) -> InlineFlowLayout {
    let line_height = text_style.line_height_in_pixels(window.rem_size());
    let rem_size = window.rem_size();
    let total_len = items.iter().map(MeasureItem::len).sum::<usize>();
    if total_len == 0 {
        return InlineFlowLayout::default();
    }

    let line_ranges = line_ranges(items, image_sizes, text_style, wrap_width, window);
    let font_size = text_style.font_size.to_pixels(rem_size);
    let mut fragments = Vec::new();
    let mut max_width = Pixels::ZERO;
    let mut y = Pixels::ZERO;

    for line_range in line_ranges {
        let mut line_fragments = Vec::new();
        let mut line_width = Pixels::ZERO;
        let mut actual_line_height = line_height;
        let mut item_start = 0;

        for (item_ix, item) in items.iter().enumerate() {
            let item_end = item_start + item.len();
            if item_end <= line_range.start {
                item_start = item_end;
                continue;
            }
            if item_start >= line_range.end {
                break;
            }

            match item {
                MeasureItem::Code { text, style } => {
                    let code_size = measure_code(text, style, wrap_width, window);
                    line_width += code_size.width;
                    actual_line_height = actual_line_height.max(code_size.height);
                    line_fragments.push(LineFragmentLayout {
                        item_ix,
                        kind: LineFragmentKind::Code,
                        size: code_size,
                        source_range: 0..IMAGE_LEN,
                    });
                }
                MeasureItem::Text {
                    text,
                    links,
                    highlights,
                } => {
                    let local_start = line_range.start.max(item_start) - item_start;
                    let local_end = line_range.end.min(item_end) - item_start;
                    if local_start < local_end {
                        let subtext = SharedString::from(text[local_start..local_end].to_string());
                        let highlights =
                            slice_ranges(highlights, local_start, local_end, |range, style| {
                                (range, *style)
                            });
                        let links = slice_ranges(links, local_start, local_end, |range, link| {
                            (range, link.clone())
                        });
                        let runs = runs_for_highlights(&subtext, text_style, highlights.clone());
                        let shaped_line = shape_line(subtext.clone(), font_size, &runs, window);
                        let width = shaped_line.width();
                        line_width += width;
                        line_fragments.push(LineFragmentLayout {
                            item_ix,
                            kind: LineFragmentKind::Text {
                                text: subtext,
                                links,
                                highlights,
                            },
                            size: size(width, line_height),
                            source_range: local_start..local_end,
                        });
                    }
                }
                MeasureItem::Image { .. } => {
                    if line_range.start <= item_start && item_end <= line_range.end {
                        let size = image_sizes[item_ix]
                            .expect("image size should be measured before layout");
                        line_width += size.width;
                        actual_line_height = actual_line_height.max(size.height);
                        line_fragments.push(LineFragmentLayout {
                            item_ix,
                            kind: LineFragmentKind::Image,
                            size,
                            source_range: 0..IMAGE_LEN,
                        });
                    }
                }
            }

            item_start = item_end;
        }

        let mut x = Pixels::ZERO;
        for fragment in line_fragments {
            let origin = point(x, y + (actual_line_height - fragment.size.height) / 2.);
            let positioned = match fragment.kind {
                LineFragmentKind::Code => PositionedFragment::Code {
                    item_ix: fragment.item_ix,
                    origin,
                    size: fragment.size,
                },
                LineFragmentKind::Text {
                    text,
                    links,
                    highlights,
                } => PositionedFragment::Text {
                    item_ix: fragment.item_ix,
                    origin,
                    size: fragment.size,
                    source_range: fragment.source_range,
                    text,
                    links,
                    highlights,
                },
                LineFragmentKind::Image => PositionedFragment::Image {
                    item_ix: fragment.item_ix,
                    origin,
                    size: fragment.size,
                },
            };
            x += fragment.size.width;
            fragments.push(positioned);
        }

        max_width = max_width.max(line_width);
        y += actual_line_height;
    }

    InlineFlowLayout {
        fragments,
        size: size(max_width, y),
    }
}

fn line_ranges(
    items: &[MeasureItem],
    image_sizes: &[Option<Size<Pixels>>],
    text_style: &TextStyle,
    wrap_width: Option<Pixels>,
    window: &mut Window,
) -> Vec<Range<usize>> {
    let total_len = items.iter().map(MeasureItem::len).sum::<usize>();
    let mut hard_lines = Vec::new();
    let mut line_start = 0;
    let mut item_start = 0;

    for item in items {
        if let MeasureItem::Text { text, .. } = item {
            for (newline, _) in text.match_indices('\n') {
                let newline = item_start + newline;
                hard_lines.push(line_start..newline);
                line_start = newline + 1;
            }
        }
        item_start += item.len();
    }
    hard_lines.push(line_start..total_len);

    let Some(wrap_width) = wrap_width else {
        return hard_lines;
    };
    let rem_size = window.rem_size();
    let font_size = text_style.font_size.to_pixels(rem_size);
    let mut wrapper = window
        .text_system()
        .line_wrapper(text_style.font(), font_size);
    let mut ranges = Vec::new();

    for hard_line in hard_lines {
        let mut item_start = 0;
        let wrap_fragments = items
            .iter()
            .enumerate()
            .filter_map(|(ix, item)| {
                let item_end = item_start + item.len();
                let fragment = if item_end <= hard_line.start || item_start >= hard_line.end {
                    None
                } else {
                    match item {
                        MeasureItem::Code { text, style } => Some(WrapLineFragment::element(
                            measure_code(text, style, Some(wrap_width), window).width,
                            IMAGE_LEN,
                        )),
                        MeasureItem::Text { text, .. } => {
                            let start = hard_line.start.max(item_start) - item_start;
                            let end = hard_line.end.min(item_end) - item_start;
                            (start < end).then(|| WrapLineFragment::text(&text[start..end]))
                        }
                        MeasureItem::Image { .. } => (hard_line.start <= item_start
                            && item_end <= hard_line.end)
                            .then(|| {
                                WrapLineFragment::element(
                                    image_sizes[ix]
                                        .expect("image size should be measured before wrapping")
                                        .width,
                                    IMAGE_LEN,
                                )
                            }),
                    }
                };
                item_start = item_end;
                fragment
            })
            .collect::<Vec<_>>();

        let boundaries = wrapper
            .wrap_line(&wrap_fragments, wrap_width)
            .map(|boundary| hard_line.start + boundary.ix.min(hard_line.len()))
            .collect::<Vec<_>>();
        let mut start = hard_line.start;

        for end in boundaries {
            if start < end {
                ranges.push(start..end);
            }
            start = end;
        }

        if start < hard_line.end || hard_line.is_empty() {
            ranges.push(start..hard_line.end);
        }
    }

    ranges
}

#[allow(clippy::too_many_arguments)]
fn measure_image_size(
    ix: usize,
    url: &SharedUri,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    line_height: Pixels,
    rem_size: Pixels,
    window: &mut Window,
    cx: &mut App,
) -> Size<Pixels> {
    let intrinsic_size = if width.is_some() && height.is_some() {
        None
    } else {
        intrinsic_image_size(ix, url, width, height, window, cx)
    };
    image_size(width, height, intrinsic_size, line_height, rem_size)
}

fn intrinsic_image_size(
    ix: usize,
    url: &SharedUri,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Size<Pixels>> {
    let mut element = img(image_source(url))
        .id(ix)
        .object_fit(ObjectFit::Contain)
        .max_w(relative(1.))
        .when_some(width, |this, width| this.w(width))
        .when_some(height, |this, height| this.h(height))
        .into_any_element();
    let measured_size = element.layout_as_root(AvailableSpace::min_size(), window, cx);

    if measured_size.width <= Pixels::ZERO || measured_size.height <= Pixels::ZERO {
        None
    } else {
        Some(measured_size)
    }
}

fn image_size(
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    intrinsic_size: Option<Size<Pixels>>,
    line_height: Pixels,
    rem_size: Pixels,
) -> Size<Pixels> {
    let base_size = AbsoluteLength::Pixels(line_height);
    match (width, height) {
        (Some(width), Some(height)) => size(
            width.to_pixels(base_size, rem_size),
            height.to_pixels(base_size, rem_size),
        ),
        (Some(width), None) => {
            let width = width.to_pixels(base_size, rem_size);
            let height = intrinsic_size
                .and_then(|intrinsic_size| {
                    (intrinsic_size.width > Pixels::ZERO && intrinsic_size.height > Pixels::ZERO)
                        .then(|| width * (intrinsic_size.height / intrinsic_size.width))
                })
                .unwrap_or(line_height);
            size(width, height)
        }
        (None, Some(height)) => {
            let height = height.to_pixels(base_size, rem_size);
            let width = intrinsic_size
                .and_then(|intrinsic_size| {
                    (intrinsic_size.width > Pixels::ZERO && intrinsic_size.height > Pixels::ZERO)
                        .then(|| height * (intrinsic_size.width / intrinsic_size.height))
                })
                .unwrap_or(height);
            size(width, height)
        }
        (None, None) => inline_image_size_for_line(intrinsic_size, line_height),
    }
}

fn inline_image_size_for_line(
    intrinsic_size: Option<Size<Pixels>>,
    line_height: Pixels,
) -> Size<Pixels> {
    let height = line_height * 0.75;
    let aspect_ratio = intrinsic_size
        .and_then(|intrinsic_size| {
            (intrinsic_size.width > Pixels::ZERO && intrinsic_size.height > Pixels::ZERO)
                .then(|| intrinsic_size.width / intrinsic_size.height)
        })
        .unwrap_or(1.);

    size((height * aspect_ratio).max(px(1.)), height.max(px(1.)))
}

fn runs_for_highlights(
    text: &str,
    default_style: &TextStyle,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut ix = 0;

    for (range, highlight) in highlights {
        if ix < range.start {
            runs.push(default_style.clone().to_run(range.start - ix));
        }
        runs.push(
            default_style
                .clone()
                .highlight(highlight)
                .to_run(range.len()),
        );
        ix = range.end;
    }

    if ix < text.len() {
        runs.push(default_style.to_run(text.len() - ix));
    }

    runs
}

fn shape_line(
    text: SharedString,
    font_size: Pixels,
    runs: &[TextRun],
    window: &mut Window,
) -> ShapedLine {
    window.text_system().shape_line(text, font_size, runs, None)
}

fn slice_ranges<T, U>(
    ranges: &[(Range<usize>, T)],
    start: usize,
    end: usize,
    map: impl Fn(Range<usize>, &T) -> U,
) -> Vec<U> {
    ranges
        .iter()
        .filter_map(|(range, value)| {
            let clipped_start = range.start.max(start);
            let clipped_end = range.end.min(end);
            (clipped_start < clipped_end)
                .then(|| map((clipped_start - start)..(clipped_end - start), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_image_without_explicit_size_scales_intrinsic_ratio_to_line_height() {
        let line_height = px(20.);
        let intrinsic_size = size(px(160.), px(40.));

        let measured = inline_image_size_for_line(Some(intrinsic_size), line_height);

        assert_eq!(measured, size(px(60.), px(15.)));
    }

    #[test]
    fn inline_image_without_intrinsic_size_uses_compact_square_fallback() {
        let measured = inline_image_size_for_line(None, px(20.));

        assert_eq!(measured, size(px(15.), px(15.)));
    }
}
