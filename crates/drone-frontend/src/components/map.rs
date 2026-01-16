//! # Map Component
//!
//! Afghanistan tactical map with drone markers using Leaflet.js.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::state::{use_app_state, Coordinates};

/// Leaflet map wrapper
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = L)]
    type Map;

    #[wasm_bindgen(js_namespace = L, js_name = map)]
    fn create_map(id: &str) -> Map;

    #[wasm_bindgen(method, js_name = setView)]
    fn set_view(this: &Map, lat_lng: &JsValue, zoom: u32) -> Map;

    #[wasm_bindgen(js_namespace = L, js_name = tileLayer)]
    fn tile_layer(url: &str, options: &JsValue) -> TileLayer;

    #[wasm_bindgen]
    type TileLayer;

    #[wasm_bindgen(method, js_name = addTo)]
    fn add_to(this: &TileLayer, map: &Map);

    #[wasm_bindgen(js_namespace = L)]
    type Marker;

    #[wasm_bindgen(js_namespace = L, js_name = marker)]
    fn create_marker(lat_lng: &JsValue, options: &JsValue) -> Marker;

    #[wasm_bindgen(method, js_name = addTo)]
    fn marker_add_to(this: &Marker, map: &Map);

    #[wasm_bindgen(method, js_name = bindPopup)]
    fn bind_popup(this: &Marker, content: &str) -> Marker;

    #[wasm_bindgen(method, js_name = setLatLng)]
    fn set_lat_lng(this: &Marker, lat_lng: &JsValue);

    #[wasm_bindgen(js_namespace = L)]
    type Polyline;

    #[wasm_bindgen(js_namespace = L, js_name = polyline)]
    fn create_polyline(lat_lngs: &JsValue, options: &JsValue) -> Polyline;

    #[wasm_bindgen(method, js_name = addTo)]
    fn polyline_add_to(this: &Polyline, map: &Map);
}

/// Afghanistan map panel
#[component]
pub fn MapPanel() -> impl IntoView {
    let state = use_app_state();
    let map_id = "tactical-map";

    // Center on Kandahar Province, Afghanistan
    let center_lat = 31.6289;
    let center_lng = 65.7372;

    // Initialize map on mount
    Effect::new(move |_| {
        // Check if Leaflet is loaded
        let window = web_sys::window().expect("no window");
        if window.get("L").is_none() {
            log::warn!("Leaflet not loaded, map will not render");
            return;
        }

        // Create map
        let map = create_map(map_id);
        let center = js_sys::Array::new();
        center.push(&JsValue::from_f64(center_lat));
        center.push(&JsValue::from_f64(center_lng));
        map.set_view(&center.into(), 7);

        // Add dark tile layer
        let tile_options = js_sys::Object::new();
        js_sys::Reflect::set(&tile_options, &"maxZoom".into(), &19.into()).unwrap();
        js_sys::Reflect::set(&tile_options, &"attribution".into(), &"© OpenStreetMap".into()).unwrap();
        
        let tiles = tile_layer(
            "https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}{r}.png",
            &tile_options.into(),
        );
        tiles.add_to(&map);

        // Add drone markers
        let drones = state.drones.get();
        for drone in drones.values() {
            let pos = &drone.position;
            let marker_pos = js_sys::Array::new();
            marker_pos.push(&JsValue::from_f64(pos.latitude));
            marker_pos.push(&JsValue::from_f64(pos.longitude));

            let marker_options = js_sys::Object::new();
            let marker = create_marker(&marker_pos.into(), &marker_options.into());
            
            let popup_content = format!(
                "<b>{}</b><br/>Alt: {:.0}m<br/>Hdg: {:.0}°<br/>Spd: {:.0} m/s",
                drone.callsign, pos.altitude_m, pos.heading_deg, pos.speed_mps
            );
            marker.bind_popup(&popup_content);
            marker.marker_add_to(&map);
        }

        log::info!("Map initialized with {} drone markers", drones.len());
    });

    let selected_drone = move || state.selected_drone.get();
    let drone_position = move || {
        selected_drone().and_then(|id| {
            state.drones.get().get(&id).map(|d| d.position.clone())
        })
    };

    view! {
        <div class="map-container">
            <div id=map_id style="width: 100%; height: 100%; min-height: 400px;"></div>

            <div class="map-overlay">
                <div class="map-control">
                    <span class="status-dot nominal"></span>
                    "KANDAHAR AOR"
                </div>

                {move || drone_position().map(|pos| view! {
                    <div class="map-control">
                        <span class="text-accent">"SEL:"</span>
                        {format!("{:.4}°N {:.4}°E", pos.latitude, pos.longitude)}
                    </div>
                })}
            </div>

            <div style="position: absolute; bottom: 16px; right: 16px; z-index: 100;">
                <div class="map-control" style="font-size: 0.7rem;">
                    <span class="text-muted">"ALT:"</span>
                    {move || drone_position().map(|p| format!("{:.0}m", p.altitude_m)).unwrap_or_else(|| "---".to_string())}
                    " "
                    <span class="text-muted">"HDG:"</span>
                    {move || drone_position().map(|p| format!("{:.0}°", p.heading_deg)).unwrap_or_else(|| "---".to_string())}
                </div>
            </div>
        </div>
    }
}

/// Map fallback when Leaflet not loaded
#[component]
pub fn MapFallback() -> impl IntoView {
    view! {
        <div class="map-container" style="display: flex; align-items: center; justify-content: center;">
            <div style="text-align: center;">
                <div class="spinner" style="margin: 0 auto 16px;"></div>
                <div class="text-muted">"Loading tactical map..."</div>
            </div>
        </div>
    }
}
