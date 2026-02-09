use leptos::prelude::*;

/// A vertical drag handle placed between two horizontal panels.
///
/// Renders a thin vertical bar that expands on hover, with a small grab
/// indicator (three dots) visible on hover. Supports click-drag to resize
/// the left panel and double-click to reset to the initial width.
#[component]
pub fn ResizeDivider(
    #[prop(default = 200)] min_px: u32,
    #[prop(default = 400)] max_px: u32,
    #[prop(default = 256)] initial_px: u32,
    width: WriteSignal<u32>,
) -> impl IntoView {
    let (dragging, set_dragging) = signal(false);
    let (hovered, set_hovered) = signal(false);

    // Shared drag state: start_x and width snapshot at mousedown.
    let (drag_start_x, set_drag_start_x) = signal(0i32);
    let (drag_start_width, set_drag_start_width) = signal(initial_px);

    // --- hydrate-only: attach persistent global listeners once via Effect ---
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            use wasm_bindgen::prelude::*;
            use wasm_bindgen::JsCast;

            // Attach global mousemove + mouseup listeners once on mount.
            Effect::new(move |_| {
                let win = match web_sys::window() {
                    Some(w) => w,
                    None => return,
                };

                // --- mousemove ---
                let move_cb = Closure::wrap(Box::new(move |ev: web_sys::Event| {
                    if !dragging.get() {
                        return;
                    }
                    let client_x = js_sys::Reflect::get(&ev, &JsValue::from_str("clientX"))
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0) as i32;

                    let delta = client_x - drag_start_x.get();
                    let new_raw = (drag_start_width.get() as i32 + delta).max(0) as u32;
                    let clamped = new_raw.clamp(min_px, max_px);
                    width.set(clamped);
                }) as Box<dyn FnMut(web_sys::Event)>);

                let _ = win.add_event_listener_with_callback(
                    "mousemove",
                    move_cb.as_ref().unchecked_ref(),
                );
                move_cb.forget();

                // --- mouseup ---
                let up_cb = Closure::wrap(Box::new(move |_ev: web_sys::Event| {
                    if !dragging.get() {
                        return;
                    }
                    set_dragging.set(false);

                    // Re-enable text selection.
                    if let Some(body) = document().body() {
                        let _ = body.class_list().remove_1("select-none");
                    }
                }) as Box<dyn FnMut(web_sys::Event)>);

                let _ = win.add_event_listener_with_callback(
                    "mouseup",
                    up_cb.as_ref().unchecked_ref(),
                );
                up_cb.forget();
            });

            let on_mousedown = move |ev: leptos::ev::MouseEvent| {
                ev.prevent_default();

                // Snapshot the current width from the previous sibling element.
                let current_width: u32 = {
                    let target: web_sys::EventTarget = ev.target().unwrap();
                    let el: web_sys::Element = target.unchecked_into();
                    // The hit target is a child of the divider. Walk up to the
                    // divider, then read its previous sibling (the sidebar).
                    let divider = el.closest("[data-resize-divider]")
                        .ok()
                        .flatten();
                    if let Some(div_el) = divider {
                        if let Some(prev) = div_el.previous_element_sibling() {
                            let html: &web_sys::HtmlElement = prev.unchecked_ref();
                            html.offset_width() as u32
                        } else {
                            initial_px
                        }
                    } else {
                        initial_px
                    }
                };

                set_drag_start_x.set(ev.client_x());
                set_drag_start_width.set(current_width);
                set_dragging.set(true);

                // Disable text selection while dragging.
                if let Some(body) = document().body() {
                    let _ = body.class_list().add_1("select-none");
                }
            };

            let on_dblclick = move |_ev: leptos::ev::MouseEvent| {
                width.set(initial_px);
            };
        } else {
            // Suppress unused-variable warnings under SSR where drag logic is inert.
            let _ = (min_px, max_px, width, set_dragging, set_drag_start_x, set_drag_start_width);
            let _ = drag_start_x;
            let _ = drag_start_width;
        }
    }

    view! {
        <div
            data-resize-divider=""
            class=move || format!(
                "relative flex-shrink-0 cursor-col-resize transition-all duration-100 group {}",
                if dragging.get() {
                    "w-1.5 bg-amber-500/40"
                } else if hovered.get() {
                    "w-1.5 bg-amber-500/30"
                } else {
                    "w-0.5 bg-zinc-800/40"
                }
            )
            on:mouseenter=move |_| set_hovered.set(true)
            on:mouseleave=move |_| set_hovered.set(false)
        >
            // Wider invisible hit target so the thin bar is easy to grab.
            <div
                class="absolute inset-y-0 -left-1 -right-1 z-10"

                on:mousedown={
                    cfg_if::cfg_if! {
                        if #[cfg(feature = "hydrate")] {
                            on_mousedown
                        } else {
                            move |_ev: leptos::ev::MouseEvent| {}
                        }
                    }
                }

                on:dblclick={
                    cfg_if::cfg_if! {
                        if #[cfg(feature = "hydrate")] {
                            on_dblclick
                        } else {
                            move |_ev: leptos::ev::MouseEvent| {}
                        }
                    }
                }
            ></div>

            // Grab indicator: three small dots, visible on hover / during drag.
            <div class=move || format!(
                "absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 flex flex-col gap-0.5 transition-opacity duration-100 {}",
                if hovered.get() || dragging.get() { "opacity-100" } else { "opacity-0" }
            )>
                <div class="w-1 h-1 rounded-full bg-zinc-500"></div>
                <div class="w-1 h-1 rounded-full bg-zinc-500"></div>
                <div class="w-1 h-1 rounded-full bg-zinc-500"></div>
            </div>
        </div>
    }
}
