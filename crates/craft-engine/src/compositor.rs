//! Native Compositor and Surface manager for WebRender / Servo rendering in CraftBrowsing.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;

/// Surface dimensions and high-DPI scaling configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceMetrics {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

impl Default for SurfaceMetrics {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            scale_factor: 1.0,
        }
    }
}

/// Represents the platform-agnostic compositing surface backing a Servo tab.
pub struct CompositorSurface {
    metrics: SurfaceMetrics,
    dirty: Arc<AtomicBool>,
    vsync_enabled: bool,
}

impl CompositorSurface {
    /// Initialize a new compositing surface with target dimensions.
    pub fn new(width: u32, height: u32, scale_factor: f64) -> Self {
        info!(
            "Initializing CompositorSurface: {}x{} @ {}x scale factor",
            width, height, scale_factor
        );
        Self {
            metrics: SurfaceMetrics {
                width,
                height,
                scale_factor,
            },
            dirty: Arc::new(AtomicBool::new(true)),
            vsync_enabled: true,
        }
    }

    /// Resize the backing surface and mark dirty for re-composite.
    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f64) {
        if self.metrics.width != width
            || self.metrics.height != height
            || (self.metrics.scale_factor - scale_factor).abs() > f64::EPSILON
        {
            self.metrics = SurfaceMetrics {
                width,
                height,
                scale_factor,
            };
            self.mark_dirty();
            info!(
                "CompositorSurface resized: {}x{} (scale: {:.2})",
                width, height, scale_factor
            );
        }
    }

    /// Mark surface as needing repaint.
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    /// Check if the surface requires a frame render and reset dirty flag.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    /// Surface metrics getter.
    pub fn metrics(&self) -> SurfaceMetrics {
        self.metrics
    }

    /// Toggle hardware VSync.
    pub fn set_vsync(&mut self, enabled: bool) {
        self.vsync_enabled = enabled;
    }

    pub fn vsync_enabled(&self) -> bool {
        self.vsync_enabled
    }
}
