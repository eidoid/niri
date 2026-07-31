use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::{
    import_surface, Buffer as RendererBuffer, CommitCounter, DamageSet, OpaqueRegions,
    RendererSurfaceStateUserData, SurfaceView,
};
use smithay::backend::renderer::{ImportAll, Renderer, Texture};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::user_data::UserDataMap;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Size, Transform};
use smithay::wayland::compositor::{with_surface_tree_downward, TraversalAction};

use super::texture::TextureBuffer;
use super::BakedBuffer;

/// A Wayland surface render element scaled around a window-local origin.
#[derive(Debug)]
pub struct ScaledSurfaceRenderElement<R: Renderer> {
    inner: WaylandSurfaceRenderElement<R>,
    origin: Point<i32, Physical>,
    scale: Scale<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct SurfaceRenderScaling {
    origin: Point<i32, Physical>,
    output_scale: Scale<f64>,
    content_scale: f64,
}

impl SurfaceRenderScaling {
    pub fn new(origin: Point<i32, Physical>, output_scale: Scale<f64>, content_scale: f64) -> Self {
        Self {
            origin,
            output_scale,
            content_scale,
        }
    }
}

impl<R: Renderer + ImportAll> ScaledSurfaceRenderElement<R> {
    pub fn new(
        inner: WaylandSurfaceRenderElement<R>,
        origin: Point<i32, Physical>,
        scale: f64,
    ) -> Self {
        Self {
            inner,
            origin,
            scale: Scale::from(scale),
        }
    }

    pub fn buffer_size(&self) -> Size<i32, Logical> {
        self.inner.buffer_size()
    }

    pub fn view(&self) -> SurfaceView {
        self.inner.view()
    }

    pub fn buffer(&self) -> &RendererBuffer {
        self.inner.buffer()
    }
}

impl<R: Renderer + ImportAll> Element for ScaledSurfaceRenderElement<R> {
    fn id(&self) -> &Id {
        self.inner.id()
    }

    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        let mut geometry = self.inner.geometry(scale);
        geometry.loc -= self.origin;
        geometry = geometry.to_f64().upscale(self.scale).to_i32_round();
        geometry.loc += self.origin;
        geometry
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        self.inner.src()
    }

    fn transform(&self) -> Transform {
        self.inner.transform()
    }

    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.inner
            .damage_since(scale, commit)
            .into_iter()
            .map(|rect| rect.to_f64().upscale(self.scale).to_i32_up())
            .collect()
    }

    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        self.inner
            .opaque_regions(scale)
            .into_iter()
            .map(|rect| rect.to_f64().upscale(self.scale).to_i32_round())
            .collect()
    }

    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }

    fn kind(&self) -> Kind {
        self.inner.kind()
    }

    fn is_framebuffer_effect(&self) -> bool {
        self.inner.is_framebuffer_effect()
    }
}

impl<R> RenderElement<R> for ScaledSurfaceRenderElement<R>
where
    R: Renderer + ImportAll,
    R::TextureId: Texture + 'static,
{
    fn draw(
        &self,
        frame: &mut R::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), R::Error> {
        self.inner
            .draw(frame, src, dst, damage, opaque_regions, cache)
    }

    fn underlying_storage(&self, renderer: &mut R) -> Option<UnderlyingStorage<'_>> {
        self.inner.underlying_storage(renderer)
    }

    fn capture_framebuffer(
        &self,
        frame: &mut R::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        cache: &UserDataMap,
    ) -> Result<(), R::Error> {
        self.inner.capture_framebuffer(frame, src, dst, cache)
    }
}

/// Renders elements from a surface tree as textures into `storage`.
pub fn render_snapshot_from_surface_tree(
    renderer: &mut GlesRenderer,
    surface: &WlSurface,
    location: Point<f64, Logical>,
    content_scale: f64,
    storage: &mut Vec<BakedBuffer<TextureBuffer<GlesTexture>>>,
) {
    let _span = tracy_client::span!("render_snapshot_from_surface_tree");

    with_surface_tree_downward(
        surface,
        location,
        |_, states, location| {
            let mut location = *location;
            let data = states.data_map.get::<RendererSurfaceStateUserData>();

            if let Some(data) = data {
                let data = &*data.lock().unwrap();

                if let Some(view) = data.view() {
                    location += view.offset.to_f64();
                    TraversalAction::DoChildren(location)
                } else {
                    TraversalAction::SkipChildren
                }
            } else {
                TraversalAction::SkipChildren
            }
        },
        |_, states, location| {
            let mut location = *location;
            let data = states.data_map.get::<RendererSurfaceStateUserData>();

            if let Some(data) = data {
                let Some(view) = data.lock().unwrap().view() else {
                    return;
                };
                location += view.offset.to_f64();

                if let Err(err) = import_surface(renderer, states) {
                    warn!("failed to import surface: {err:?}");
                    return;
                }

                let data = data.lock().unwrap();
                let Some(texture) = data.texture(renderer.context_id()) else {
                    return;
                };

                let buffer = TextureBuffer::from_texture(
                    renderer,
                    texture.clone(),
                    f64::from(data.buffer_scale()),
                    data.buffer_transform(),
                    Vec::new(),
                );

                let baked = BakedBuffer {
                    buffer,
                    location: location.upscale(content_scale),
                    src: Some(view.src),
                    dst: Some(view.dst.to_f64().upscale(content_scale).to_i32_round()),
                };

                storage.push(baked);
            }
        },
        |_, _, _| true,
    );
}

pub fn push_elements_from_surface_tree<R>(
    renderer: &mut R,
    surface: &WlSurface,
    // Fractional scale expects surface buffers to be aligned to physical pixels.
    location: Point<i32, Physical>,
    scale: Scale<f64>,
    alpha: f32,
    kind: Kind,
    push: &mut dyn FnMut(WaylandSurfaceRenderElement<R>),
) where
    R: Renderer + ImportAll,
    R::TextureId: Clone + 'static,
{
    let _span = tracy_client::span!("push_elements_from_surface_tree");

    let location = location.to_f64();

    with_surface_tree_downward(
        surface,
        location,
        |_, states, location| {
            let mut location = *location;
            let data = states.data_map.get::<RendererSurfaceStateUserData>();

            if let Some(data) = data {
                if let Some(view) = data.lock().unwrap().view() {
                    location += view.offset.to_f64().to_physical(scale);
                    TraversalAction::DoChildren(location)
                } else {
                    TraversalAction::SkipChildren
                }
            } else {
                TraversalAction::SkipChildren
            }
        },
        |surface, states, location| {
            let mut location = *location;
            let data = states.data_map.get::<RendererSurfaceStateUserData>();

            if let Some(data) = data {
                let has_view = if let Some(view) = data.lock().unwrap().view() {
                    location += view.offset.to_f64().to_physical(scale);
                    true
                } else {
                    false
                };

                if has_view {
                    match WaylandSurfaceRenderElement::from_surface(
                        renderer, surface, states, location, alpha, kind,
                    ) {
                        Ok(Some(surface)) => push(surface),
                        Ok(None) => {} // surface is not mapped
                        Err(err) => {
                            warn!("failed to import surface: {}", err);
                        }
                    };
                }
            }
        },
        |_, _, _| true,
    );
}

pub fn push_scaled_elements_from_surface_tree<R>(
    renderer: &mut R,
    surface: &WlSurface,
    location: Point<i32, Physical>,
    scaling: SurfaceRenderScaling,
    alpha: f32,
    kind: Kind,
    push: &mut dyn FnMut(ScaledSurfaceRenderElement<R>),
) where
    R: Renderer + ImportAll,
    R::TextureId: Clone + 'static,
{
    push_elements_from_surface_tree(
        renderer,
        surface,
        location,
        scaling.output_scale,
        alpha,
        kind,
        &mut |element| {
            push(ScaledSurfaceRenderElement::new(
                element,
                scaling.origin,
                scaling.content_scale,
            ));
        },
    );
}
