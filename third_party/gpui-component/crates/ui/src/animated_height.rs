//! Measured content: layout changes retarget a spring instead of jumping.
//! The natural content is measured independently of the visible clipping height.
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window, px, relative, size,
};

/// A clipped, naturally measured region with spring-animated height changes.
pub struct AnimatedHeight {
    id: ElementId,
    child: AnyElement,
    expanded: bool,
}

impl AnimatedHeight {
    pub fn new(id: impl Into<ElementId>, child: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            child: child.into_any_element(),
            expanded: true,
        }
    }

    /// Collapse to zero while retaining content through the closing animation.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }
}
impl IntoElement for AnimatedHeight {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for AnimatedHeight {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let height = window.with_element_state(id.unwrap(), |state: Option<Option<Pixels>>, _| {
            let height = state.flatten();
            (height, height)
        });
        let height = gpui_base::motion::spring(
            self.id.clone(),
            if self.expanded {
                f32::from(height.unwrap_or(px(0.)))
            } else {
                0.
            },
            gpui_base::motion::Spring::new(std::time::Duration::from_millis(220)),
            window,
            cx,
        );
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(height.max(0.)).into();
        style.flex_shrink = 0.;
        (window.request_layout(style, None, cx), ())
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let measured = self.child.layout_as_root(
            size(
                AvailableSpace::Definite(bounds.size.width),
                AvailableSpace::MinContent,
            ),
            window,
            cx,
        );
        let changed = window.with_element_state(id.unwrap(), |state: Option<Option<Pixels>>, _| {
            (
                state.flatten() != Some(measured.height),
                Some(measured.height),
            )
        });
        if changed {
            window.request_animation_frame();
        }
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.prepaint_at(bounds.origin, window, cx)
        });
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.paint(window, cx)
        });
    }
}
