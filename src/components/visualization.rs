use leptos::prelude::*;

/// Checks if a model output JSON contains spatial feature maps.
/// Looks for arrays with shape [1, C, H, W] where H*W > 16.
pub fn has_spatial_features(output: &serde_json::Value) -> bool {
    match output {
        serde_json::Value::Object(map) => {
            for value in map.values() {
                if check_spatial_value(value) {
                    return true;
                }
            }
            false
        }
        _ => check_spatial_value(output),
    }
}

/// Recursively checks a JSON value for spatial feature map shapes.
fn check_spatial_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            // Look for a "shape" key with value [1, C, H, W]
            if let Some(shape) = map.get("shape") {
                if let Some(dims) = shape.as_array() {
                    if dims.len() == 4 {
                        let batch = dims[0].as_i64().unwrap_or(0);
                        let h = dims[2].as_i64().unwrap_or(0);
                        let w = dims[3].as_i64().unwrap_or(0);
                        if batch == 1 && h * w > 16 {
                            return true;
                        }
                    }
                }
            }
            // Check nested values
            for v in map.values() {
                if check_spatial_value(v) {
                    return true;
                }
            }
            false
        }
        serde_json::Value::Array(arr) => {
            // Could be a nested array representing [1, C, H, W] data
            // Check if this is a 4-level nested array with sufficient spatial extent
            if is_4d_nested_array(arr) {
                return true;
            }
            // Otherwise check each element
            for v in arr {
                if check_spatial_value(v) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// Checks whether an array appears to be a 4D nested numeric array [1][C][H][W]
/// where H*W > 16.
fn is_4d_nested_array(arr: &[serde_json::Value]) -> bool {
    // [1, C, H, W] means outer length = 1 (batch)
    if arr.len() != 1 {
        return false;
    }
    let channels = match arr[0].as_array() {
        Some(c) => c,
        None => return false,
    };
    if channels.is_empty() {
        return false;
    }
    // Each channel should be a 2D array (rows of cols)
    let first_channel = match channels[0].as_array() {
        Some(rows) => rows,
        None => return false,
    };
    let h = first_channel.len();
    if h == 0 {
        return false;
    }
    let w = match first_channel[0].as_array() {
        Some(cols) => cols.len(),
        None => return false,
    };
    (h * w) > 16
}

// ---------------------------------------------------------------------------
// Heatmap drawing (hydrate-only)
// ---------------------------------------------------------------------------

cfg_if::cfg_if! {
    if #[cfg(feature = "hydrate")] {
        use wasm_bindgen::JsCast;

        /// Draws the feature map as a heatmap onto the given canvas element.
        ///
        /// Values are normalized to [0, 1] and mapped to an amber-to-transparent
        /// color gradient: high activation = amber (245, 158, 11), low = transparent.
        fn draw_heatmap(
            canvas: &web_sys::HtmlCanvasElement,
            feature_map: &[f64],
            width: u32,
            height: u32,
            opacity: f64,
        ) {
            canvas.set_width(width);
            canvas.set_height(height);

            let ctx = match canvas
                .get_context("2d")
                .ok()
                .flatten()
            {
                Some(ctx) => ctx,
                None => return,
            };
            let ctx: web_sys::CanvasRenderingContext2d = match ctx.dyn_into() {
                Ok(c) => c,
                Err(_) => return,
            };

            let expected_len = (width * height) as usize;
            if feature_map.is_empty() || expected_len == 0 {
                return;
            }

            // Normalize feature map values to [0, 1]
            let min_val = feature_map.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_val = feature_map.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let range = max_val - min_val;

            let normalize = |v: f64| -> f64 {
                if range.abs() < f64::EPSILON {
                    0.0
                } else {
                    ((v - min_val) / range).clamp(0.0, 1.0)
                }
            };

            // Build RGBA pixel buffer
            let mut rgba = vec![0u8; expected_len * 4];
            for i in 0..expected_len {
                let val = if i < feature_map.len() {
                    normalize(feature_map[i])
                } else {
                    0.0
                };

                // Amber (245, 158, 11) at full activation, transparent at zero
                let alpha = (val * opacity * 255.0) as u8;
                let base = i * 4;
                rgba[base] = 245;     // R
                rgba[base + 1] = 158; // G
                rgba[base + 2] = 11;  // B
                rgba[base + 3] = alpha;
            }

            // Create ImageData and paint it
            let clamped = wasm_bindgen::Clamped(&rgba[..]);
            if let Ok(image_data) =
                web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, width, height)
            {
                let _ = ctx.put_image_data(&image_data, 0.0, 0.0);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// VisualizationOverlay component
// ---------------------------------------------------------------------------

/// Props for the `VisualizationOverlay` component.
///
/// Renders the original image with an optional heatmap overlay drawn from
/// the model's spatial feature map output.
#[component]
pub fn VisualizationOverlay(
    /// Base-64 encoded image data (without the `data:` prefix).
    image_base64: String,
    /// MIME type for the image, e.g. `"image/png"`.
    image_mime: String,
    /// Flattened feature map values (row-major, length = width * height).
    feature_map: Vec<f64>,
    /// Width of the feature map (and canvas overlay) in pixels.
    width: u32,
    /// Height of the feature map (and canvas overlay) in pixels.
    height: u32,
) -> impl IntoView {
    let (show_overlay, set_show_overlay) = signal(false);
    let (opacity_pct, set_opacity_pct) = signal(60u32);

    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();

    // Build the image data-URI once
    let img_src = format!("data:{};base64,{}", image_mime, image_base64);

    // Re-draw the heatmap whenever the overlay is toggled or opacity changes (hydrate-only)
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            let feature_map_draw = feature_map.clone();
            Effect::new(move || {
                let visible = show_overlay.get();
                let pct = opacity_pct.get();

                if let Some(canvas_el) = canvas_ref.get() {
                    if visible {
                        let opacity = f64::from(pct) / 100.0;
                        let html_canvas: &web_sys::HtmlCanvasElement = &canvas_el;
                        draw_heatmap(html_canvas, &feature_map_draw, width, height, opacity);
                    } else {
                        // Clear the canvas when overlay is hidden
                        let html_canvas: &web_sys::HtmlCanvasElement = &canvas_el;
                        html_canvas.set_width(width);
                        html_canvas.set_height(height);
                        if let Some(ctx) = html_canvas
                            .get_context("2d")
                            .ok()
                            .flatten()
                        {
                            if let Ok(ctx) = ctx.dyn_into::<web_sys::CanvasRenderingContext2d>() {
                                ctx.clear_rect(0.0, 0.0, f64::from(width), f64::from(height));
                            }
                        }
                    }
                }
            });
        } else {
            // Suppress unused variable warning in SSR mode
            let _ = &feature_map;
        }
    }

    view! {
        <div class="space-y-2">
            // Image container with overlay
            <div class="relative inline-block">
                <img
                    src=img_src
                    alt="Input image"
                    class="rounded-lg max-h-80 object-contain"
                    style=format!("width: {}px; height: {}px;", width, height)
                />
                // Canvas overlay -- always in the DOM so NodeRef is available;
                // visibility is toggled via the `hidden` class on SSR and via
                // the drawing effect on hydrate.
                <canvas
                    node_ref=canvas_ref
                    class="absolute inset-0 rounded-lg pointer-events-none"
                    class:hidden=move || !show_overlay.get()
                    width=width
                    height=height
                    style=format!("width: {}px; height: {}px;", width, height)
                />
            </div>

            // Controls row
            <div class="flex items-center gap-3 text-xs text-zinc-400 select-none">
                <button
                    on:click=move |_| set_show_overlay.update(|v| *v = !*v)
                    class="px-2.5 py-1 rounded-md transition-colors duration-150 cursor-pointer"
                    class=("bg-amber-600/80 text-zinc-50", move || show_overlay.get())
                    class=("bg-zinc-700/60 text-zinc-400 hover:bg-zinc-700", move || !show_overlay.get())
                >
                    {move || if show_overlay.get() { "Hide overlay" } else { "Visualize" }}
                </button>

                {move || {
                    if show_overlay.get() {
                        Some(view! {
                            <label class="flex items-center gap-2">
                                <span class="text-zinc-500">"Opacity"</span>
                                <input
                                    type="range"
                                    min="0"
                                    max="100"
                                    prop:value=move || opacity_pct.get().to_string()
                                    on:input=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                            set_opacity_pct.set(v);
                                        }
                                    }
                                    class="w-24 accent-amber-500"
                                />
                                <span class="tabular-nums w-8 text-right">{move || format!("{}%", opacity_pct.get())}</span>
                            </label>
                        })
                    } else {
                        None
                    }
                }}
            </div>
        </div>
    }
}
