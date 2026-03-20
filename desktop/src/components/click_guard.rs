use dioxus::prelude::*;

const CLICK_DRAG_TOLERANCE_PX: f64 = 5.0;

pub fn capture_mouse_down(mut down_pos: Signal<(f64, f64)>, evt: Event<MouseData>) {
    let coords = evt.client_coordinates();
    down_pos.set((coords.x, coords.y));
}

pub fn is_click_without_drag(down_pos: Signal<(f64, f64)>, evt: &Event<MouseData>) -> bool {
    let coords = evt.client_coordinates();
    let (down_x, down_y) = *down_pos.read();

    (coords.x - down_x).abs() < CLICK_DRAG_TOLERANCE_PX
        && (coords.y - down_y).abs() < CLICK_DRAG_TOLERANCE_PX
}
