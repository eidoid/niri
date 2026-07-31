use std::borrow::Cow;
use std::ops::Deref;
use std::sync::Arc;

use smithay::input::dnd::{DndFocus, Source};
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
    GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
    GestureSwipeUpdateEvent, MotionEvent, PointerTarget, RelativeMotionEvent,
};
use smithay::input::touch::{
    DownEvent, MotionEvent as TouchMotionEvent, OrientationEvent, ShapeEvent, TouchTarget, UpEvent,
};
use smithay::input::Seat;
use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{DisplayHandle, Resource};
use smithay::utils::{IsAlive, Logical, Point, Serial};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::selection::data_device::WlOfferData;

use crate::niri::State;

/// A Wayland input target whose incoming surface-local coordinates are scaled.
#[derive(Debug, Clone)]
pub struct SurfaceFocusTarget {
    surface: WlSurface,
    scale: f64,
}

impl SurfaceFocusTarget {
    pub fn new(surface: WlSurface, scale: f64) -> Self {
        Self { surface, scale }
    }

    pub fn surface(&self) -> &WlSurface {
        &self.surface
    }

    pub fn id(&self) -> ObjectId {
        self.surface.id()
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    pub fn to_surface_point(&self, point: Point<f64, Logical>) -> Point<f64, Logical> {
        point.downscale(self.scale)
    }
}

impl From<WlSurface> for SurfaceFocusTarget {
    fn from(surface: WlSurface) -> Self {
        Self::new(surface, 1.)
    }
}

impl Deref for SurfaceFocusTarget {
    type Target = WlSurface;

    fn deref(&self) -> &Self::Target {
        &self.surface
    }
}

// Scale changes should update coordinates without causing a synthetic leave and re-enter.
impl PartialEq for SurfaceFocusTarget {
    fn eq(&self, other: &Self) -> bool {
        self.surface == other.surface
    }
}

impl PartialEq<WlSurface> for SurfaceFocusTarget {
    fn eq(&self, other: &WlSurface) -> bool {
        &self.surface == other
    }
}

impl IsAlive for SurfaceFocusTarget {
    fn alive(&self) -> bool {
        self.surface.alive()
    }
}

impl WaylandFocus for SurfaceFocusTarget {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        Some(Cow::Borrowed(&self.surface))
    }

    fn same_client_as(&self, object_id: &ObjectId) -> bool {
        self.surface.id().same_client_as(object_id)
    }
}

impl PointerTarget<State> for SurfaceFocusTarget {
    fn enter(&self, seat: &Seat<State>, data: &mut State, event: &MotionEvent) {
        let mut event = event.clone();
        event.location = self.to_surface_point(event.location);
        PointerTarget::enter(&self.surface, seat, data, &event);
    }

    fn motion(&self, seat: &Seat<State>, data: &mut State, event: &MotionEvent) {
        let mut event = event.clone();
        event.location = self.to_surface_point(event.location);
        PointerTarget::motion(&self.surface, seat, data, &event);
    }

    fn relative_motion(&self, seat: &Seat<State>, data: &mut State, event: &RelativeMotionEvent) {
        let mut event = event.clone();
        event.delta = event.delta.downscale(self.scale);
        event.delta_unaccel = event.delta_unaccel.downscale(self.scale);
        PointerTarget::relative_motion(&self.surface, seat, data, &event);
    }

    fn button(&self, seat: &Seat<State>, data: &mut State, event: &ButtonEvent) {
        PointerTarget::button(&self.surface, seat, data, event);
    }

    fn axis(&self, seat: &Seat<State>, data: &mut State, mut frame: AxisFrame) {
        frame.axis.0 /= self.scale;
        frame.axis.1 /= self.scale;
        PointerTarget::axis(&self.surface, seat, data, frame);
    }

    fn frame(&self, seat: &Seat<State>, data: &mut State) {
        PointerTarget::frame(&self.surface, seat, data);
    }

    fn leave(&self, seat: &Seat<State>, data: &mut State, serial: Serial, time: u32) {
        PointerTarget::leave(&self.surface, seat, data, serial, time);
    }

    fn gesture_swipe_begin(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &GestureSwipeBeginEvent,
    ) {
        PointerTarget::gesture_swipe_begin(&self.surface, seat, data, event);
    }

    fn gesture_swipe_update(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &GestureSwipeUpdateEvent,
    ) {
        let mut event = event.clone();
        event.delta = event.delta.downscale(self.scale);
        PointerTarget::gesture_swipe_update(&self.surface, seat, data, &event);
    }

    fn gesture_swipe_end(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &GestureSwipeEndEvent,
    ) {
        PointerTarget::gesture_swipe_end(&self.surface, seat, data, event);
    }

    fn gesture_pinch_begin(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &GesturePinchBeginEvent,
    ) {
        PointerTarget::gesture_pinch_begin(&self.surface, seat, data, event);
    }

    fn gesture_pinch_update(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &GesturePinchUpdateEvent,
    ) {
        let mut event = event.clone();
        event.delta = event.delta.downscale(self.scale);
        PointerTarget::gesture_pinch_update(&self.surface, seat, data, &event);
    }

    fn gesture_pinch_end(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &GesturePinchEndEvent,
    ) {
        PointerTarget::gesture_pinch_end(&self.surface, seat, data, event);
    }

    fn gesture_hold_begin(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &GestureHoldBeginEvent,
    ) {
        PointerTarget::gesture_hold_begin(&self.surface, seat, data, event);
    }

    fn gesture_hold_end(&self, seat: &Seat<State>, data: &mut State, event: &GestureHoldEndEvent) {
        PointerTarget::gesture_hold_end(&self.surface, seat, data, event);
    }
}

impl TouchTarget<State> for SurfaceFocusTarget {
    fn down(&self, seat: &Seat<State>, data: &mut State, event: &DownEvent, seq: Serial) {
        let mut event = event.clone();
        event.location = self.to_surface_point(event.location);
        TouchTarget::down(&self.surface, seat, data, &event, seq);
    }

    fn up(&self, seat: &Seat<State>, data: &mut State, event: &UpEvent, seq: Serial) {
        TouchTarget::up(&self.surface, seat, data, event, seq);
    }

    fn motion(&self, seat: &Seat<State>, data: &mut State, event: &TouchMotionEvent, seq: Serial) {
        let mut event = event.clone();
        event.location = self.to_surface_point(event.location);
        TouchTarget::motion(&self.surface, seat, data, &event, seq);
    }

    fn frame(&self, seat: &Seat<State>, data: &mut State, seq: Serial) {
        TouchTarget::frame(&self.surface, seat, data, seq);
    }

    fn cancel(&self, seat: &Seat<State>, data: &mut State, seq: Serial) {
        TouchTarget::cancel(&self.surface, seat, data, seq);
    }

    fn shape(&self, seat: &Seat<State>, data: &mut State, event: &ShapeEvent, seq: Serial) {
        let mut event = *event;
        event.major /= self.scale;
        event.minor /= self.scale;
        TouchTarget::shape(&self.surface, seat, data, &event, seq);
    }

    fn orientation(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        event: &OrientationEvent,
        seq: Serial,
    ) {
        TouchTarget::orientation(&self.surface, seat, data, event, seq);
    }
}

impl DndFocus<State> for SurfaceFocusTarget {
    type OfferData<S: Source> = WlOfferData<S>;

    fn enter<S: Source>(
        &self,
        data: &mut State,
        dh: &DisplayHandle,
        source: Arc<S>,
        seat: &Seat<State>,
        location: Point<f64, Logical>,
        serial: &Serial,
    ) -> Option<Self::OfferData<S>> {
        DndFocus::enter(
            &self.surface,
            data,
            dh,
            source,
            seat,
            self.to_surface_point(location),
            serial,
        )
    }

    fn motion<S: Source>(
        &self,
        data: &mut State,
        offer: Option<&mut Self::OfferData<S>>,
        seat: &Seat<State>,
        location: Point<f64, Logical>,
        time: u32,
    ) {
        DndFocus::motion(
            &self.surface,
            data,
            offer,
            seat,
            self.to_surface_point(location),
            time,
        );
    }

    fn leave<S: Source>(
        &self,
        data: &mut State,
        offer: Option<&mut Self::OfferData<S>>,
        seat: &Seat<State>,
    ) {
        DndFocus::leave(&self.surface, data, offer, seat);
    }

    fn drop<S: Source>(
        &self,
        data: &mut State,
        offer: Option<&mut Self::OfferData<S>>,
        seat: &Seat<State>,
    ) {
        DndFocus::drop(&self.surface, data, offer, seat);
    }
}
