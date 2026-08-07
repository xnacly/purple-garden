use std::ffi::CString;

use purple_garden_macros::{GardenValue, pg_pkg};
use purple_garden_runtime::{Pkg, Vm};

#[repr(C)]
#[derive(Clone, Copy)]
struct CColor {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[derive(GardenValue)]
pub struct Color {
    pub r: i64,
    pub g: i64,
    pub b: i64,
    pub a: i64,
}

impl From<Color> for CColor {
    fn from(c: Color) -> Self {
        Self {
            r: c.r as u8,
            g: c.g as u8,
            b: c.b as u8,
            a: c.a as u8,
        }
    }
}

unsafe extern "C" {
    fn InitWindow(width: i32, height: i32, title: *const std::ffi::c_char);
    fn CloseWindow();
    fn WindowShouldClose() -> bool;
    fn SetTargetFPS(fps: i32);
    fn BeginDrawing();
    fn EndDrawing();
    fn ClearBackground(color: CColor);
    fn DrawText(
        text: *const std::ffi::c_char,
        pos_x: i32,
        pos_y: i32,
        font_size: i32,
        color: CColor,
    );
    fn MeasureText(text: *const std::ffi::c_char, font_size: i32) -> i32;
    fn DrawRectangle(pos_x: i32, pos_y: i32, width: i32, height: i32, color: CColor);
}

#[pg_pkg(runtime = purple_garden_runtime)]
/// A small experimental raylib integration.
pub mod raylib {
    use super::*;

    #[pg_fn(unsafe)]
    pub fn init(vm: &mut Vm, width: i64, height: i64, title: String) {
        let title = CString::new(title).unwrap_or_else(|_| CString::new("raylib").unwrap());
        unsafe { InitWindow(width as i32, height as i32, title.as_ptr()) };
        let _ = vm;
    }

    #[pg_fn(unsafe)]
    pub fn close(_: &mut Vm) {
        unsafe { CloseWindow() };
    }

    #[pg_fn(unsafe)]
    pub fn should_close(_: &mut Vm) -> bool {
        unsafe { WindowShouldClose() }
    }

    #[pg_fn(unsafe)]
    pub fn set_target_fps(_: &mut Vm, fps: i64) {
        unsafe { SetTargetFPS(fps as i32) };
    }

    #[pg_fn(unsafe)]
    pub fn begin_drawing(_: &mut Vm) {
        unsafe { BeginDrawing() };
    }

    #[pg_fn(unsafe)]
    pub fn end_drawing(_: &mut Vm) {
        unsafe { EndDrawing() };
    }

    #[pg_fn(unsafe)]
    pub fn clear_background(_: &mut Vm, color: Color) {
        unsafe { ClearBackground(color.into()) };
    }

    #[pg_fn(unsafe)]
    pub fn draw_text(_: &mut Vm, text: String, x: i64, y: i64, size: i64, color: Color) {
        let text = CString::new(text).unwrap_or_default();
        unsafe { DrawText(text.as_ptr(), x as i32, y as i32, size as i32, color.into()) };
    }

    #[pg_fn(unsafe)]
    pub fn measure_text(_: &mut Vm, text: String, size: i64) -> i64 {
        let text = CString::new(text).unwrap_or_default();
        unsafe { MeasureText(text.as_ptr(), size as i32) as i64 }
    }

    #[pg_fn(unsafe)]
    pub fn draw_rectangle(_: &mut Vm, x: i64, y: i64, width: i64, height: i64, color: Color) {
        unsafe {
            DrawRectangle(
                x as i32,
                y as i32,
                width as i32,
                height as i32,
                color.into(),
            )
        };
    }

    pub use super::Color;
}

pub const RAYLIB_PACKAGE: Pkg = raylib::PACKAGE;
