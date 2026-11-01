// Anti-aliased drawing for the notch and the settings window: Direct2D for
// shapes and DirectWrite for text, rendered (software, no GPU device kept
// around) into a 32-bit premultiplied-alpha DIB. The notch hands that DIB
// to UpdateLayeredWindow, so its rounded corners and the glow around it are
// smooth per-pixel alpha instead of a stair-stepped window region.

use std::ffi::c_void;

use windows::core::{Interface, PCWSTR};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::HDC;

struct Gfx {
    factory: ID2D1Factory,
    dw: IDWriteFactory,
    rt: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    round: ID2D1StrokeStyle,
    dash: ID2D1StrokeStyle,
    formats: Vec<(String, i32, i32, u8, IDWriteTextFormat)>,
    bitmap: Option<(u64, ID2D1Bitmap)>,
    layers: Vec<bool>,
    drawing: bool,
}

static mut G: Option<Gfx> = None;

fn g() -> Option<&'static mut Gfx> {
    unsafe { (*(&raw mut G)).as_mut() }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn init() -> bool {
    unsafe {
        if g().is_some() {
            return true;
        }
        let Ok(factory) = D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) else { return false };
        let Ok(dw) = DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) else { return false };
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let Ok(rt) = factory.CreateDCRenderTarget(&props) else { return false };
        let white = D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
        let Ok(brush) = rt.CreateSolidColorBrush(&white, None) else { return false };
        let sp = |dash: D2D1_DASH_STYLE| D2D1_STROKE_STYLE_PROPERTIES {
            startCap: D2D1_CAP_STYLE_ROUND,
            endCap: D2D1_CAP_STYLE_ROUND,
            dashCap: D2D1_CAP_STYLE_ROUND,
            lineJoin: D2D1_LINE_JOIN_ROUND,
            miterLimit: 10.0,
            dashStyle: dash,
            dashOffset: 0.0,
        };
        let Ok(round) = factory.CreateStrokeStyle(&sp(D2D1_DASH_STYLE_SOLID), None) else { return false };
        let Ok(dash) = factory.CreateStrokeStyle(&sp(D2D1_DASH_STYLE_CUSTOM), Some(&[2.5, 2.5])) else { return false };
        *(&raw mut G) = Some(Gfx {
            factory,
            dw,
            rt,
            brush,
            round,
            dash,
            formats: Vec::new(),
            bitmap: None,
            layers: Vec::new(),
            drawing: false,
        });
        true
    }
}

fn color(c: u32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: (c & 0xFF) as f32 / 255.0,
        g: ((c >> 8) & 0xFF) as f32 / 255.0,
        b: ((c >> 16) & 0xFF) as f32 / 255.0,
        a,
    }
}

unsafe fn brush(c: u32, a: f32) -> Option<(&'static ID2D1DCRenderTarget, &'static ID2D1SolidColorBrush)> {
    let gx = g()?;
    if !gx.drawing {
        return None;
    }
    gx.brush.SetColor(&color(c, a));
    Some((&gx.rt, &gx.brush))
}

fn rf(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + h }
}

/// Start drawing into `hdc` (a DC with a 32-bit DIB selected), `w` x `h`
/// pixels, cleared to transparent.
pub unsafe fn begin(hdc: *mut c_void, w: i32, h: i32) -> bool {
    let Some(gx) = g() else { return false };
    let r = RECT { left: 0, top: 0, right: w, bottom: h };
    if gx.rt.BindDC(HDC(hdc), &r).is_err() {
        return false;
    }
    gx.rt.BeginDraw();
    gx.rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
    gx.rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
    gx.rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
    gx.drawing = true;
    gx.layers.clear();
    transform(1.0, 1.0, 0.0, 0.0);
    true
}

pub unsafe fn end() {
    if let Some(gx) = g() {
        while let Some(is_layer) = gx.layers.pop() {
            if is_layer { gx.rt.PopLayer() } else { gx.rt.PopAxisAlignedClip() }
        }
        let _ = gx.rt.EndDraw(None, None);
        gx.drawing = false;
    }
}

/// Design units -> pixels: scale (kx, ky), then offset (dx, dy) in pixels.
pub unsafe fn transform(kx: f32, ky: f32, dx: f32, dy: f32) {
    if let Some(gx) = g() {
        let m = Matrix3x2 { M11: kx, M12: 0.0, M21: 0.0, M22: ky, M31: dx, M32: dy };
        gx.rt.SetTransform(&m);
    }
}

pub unsafe fn fill_rect(x: f32, y: f32, w: f32, h: f32, c: u32, a: f32) {
    if let Some((rt, b)) = brush(c, a) {
        rt.FillRectangle(&rf(x, y, w, h), b);
    }
}

pub unsafe fn fill_round(x: f32, y: f32, w: f32, h: f32, r: f32, c: u32, a: f32) {
    if let Some((rt, b)) = brush(c, a) {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let rr = D2D1_ROUNDED_RECT { rect: rf(x, y, w, h), radiusX: r, radiusY: r };
        rt.FillRoundedRectangle(&rr, b);
    }
}

pub unsafe fn stroke_round(x: f32, y: f32, w: f32, h: f32, r: f32, width: f32, dashed: bool, c: u32, a: f32) {
    if let Some((rt, b)) = brush(c, a) {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let rr = D2D1_ROUNDED_RECT { rect: rf(x, y, w, h), radiusX: r, radiusY: r };
        let gx = g().unwrap();
        let st = if dashed { &gx.dash } else { &gx.round };
        rt.DrawRoundedRectangle(&rr, b, width, st);
    }
}

pub unsafe fn fill_circle(cx: f32, cy: f32, r: f32, c: u32, a: f32) {
    if let Some((rt, b)) = brush(c, a) {
        let e = D2D1_ELLIPSE { point: D2D_POINT_2F { x: cx, y: cy }, radiusX: r, radiusY: r };
        rt.FillEllipse(&e, b);
    }
}

/// Arc stroke from `from_deg` (0 = 3 o'clock, clockwise) sweeping `sweep`.
pub unsafe fn arc(cx: f32, cy: f32, r: f32, from_deg: f32, sweep: f32, width: f32, c: u32, a: f32) {
    if sweep <= 0.0 {
        return;
    }
    let Some((rt, b)) = brush(c, a) else { return };
    let gx = g().unwrap();
    if sweep >= 359.9 {
        let e = D2D1_ELLIPSE { point: D2D_POINT_2F { x: cx, y: cy }, radiusX: r, radiusY: r };
        rt.DrawEllipse(&e, b, width, None);
        return;
    }
    let pt = |deg: f32| {
        let t = deg.to_radians();
        D2D_POINT_2F { x: cx + r * t.cos(), y: cy + r * t.sin() }
    };
    let Ok(path) = gx.factory.CreatePathGeometry() else { return };
    let Ok(sink) = path.Open() else { return };
    sink.BeginFigure(pt(from_deg), D2D1_FIGURE_BEGIN_HOLLOW);
    sink.AddArc(&D2D1_ARC_SEGMENT {
        point: pt(from_deg + sweep),
        size: D2D_SIZE_F { width: r, height: r },
        rotationAngle: 0.0,
        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
        arcSize: if sweep > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
    });
    sink.EndFigure(D2D1_FIGURE_END_OPEN);
    let _ = sink.Close();
    rt.DrawGeometry(&path, b, width, &gx.round);
}

unsafe fn path_of(pts: &[(f32, f32)], closed: bool) -> Option<ID2D1PathGeometry> {
    let gx = g()?;
    if pts.len() < 2 {
        return None;
    }
    let path = gx.factory.CreatePathGeometry().ok()?;
    let sink = path.Open().ok()?;
    sink.BeginFigure(
        D2D_POINT_2F { x: pts[0].0, y: pts[0].1 },
        if closed { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW },
    );
    let rest: Vec<D2D_POINT_2F> = pts[1..].iter().map(|p| D2D_POINT_2F { x: p.0, y: p.1 }).collect();
    sink.AddLines(&rest);
    sink.EndFigure(if closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
    sink.Close().ok()?;
    Some(path)
}

pub unsafe fn polygon(pts: &[(f32, f32)], c: u32, a: f32) {
    if let Some((rt, b)) = brush(c, a) {
        if let Some(p) = path_of(pts, true) {
            rt.FillGeometry(&p, b, None);
        }
    }
}

pub unsafe fn polyline(pts: &[(f32, f32)], width: f32, c: u32, a: f32) {
    if let Some((rt, b)) = brush(c, a) {
        if let Some(p) = path_of(pts, false) {
            rt.DrawGeometry(&p, b, width, &g().unwrap().round);
        }
    }
}

// align: 0 left, 1 centre, 2 right. Single line, vertically centred.
unsafe fn format(font: &str, size: i32, weight: i32, align: u8, ellipsis: bool) -> Option<IDWriteTextFormat> {
    let gx = g()?;
    let key = align | if ellipsis { 4 } else { 0 };
    for (n, s, w, k, f) in &gx.formats {
        if n == font && *s == size && *w == weight && *k == key {
            return Some(f.clone());
        }
    }
    let name = wide(font);
    let loc = wide("en-us");
    let f = gx
        .dw
        .CreateTextFormat(
            PCWSTR(name.as_ptr()),
            None,
            DWRITE_FONT_WEIGHT(weight),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size as f32,
            PCWSTR(loc.as_ptr()),
        )
        .ok()?;
    let _ = f.SetTextAlignment(match align {
        1 => DWRITE_TEXT_ALIGNMENT_CENTER,
        2 => DWRITE_TEXT_ALIGNMENT_TRAILING,
        _ => DWRITE_TEXT_ALIGNMENT_LEADING,
    });
    let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
    let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
    if ellipsis {
        if let Ok(sign) = gx.dw.CreateEllipsisTrimmingSign(&f) {
            let tr = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
            let _ = f.SetTrimming(&tr, &sign);
        }
    }
    gx.formats.push((font.to_string(), size, weight, key, f.clone()));
    Some(f)
}

pub unsafe fn text(s: &str, x: f32, y: f32, w: f32, h: f32, font: &str, size: i32, weight: i32, c: u32, a: f32, align: u8, ellipsis: bool) {
    if s.is_empty() {
        return;
    }
    let Some((rt, b)) = brush(c, a) else { return };
    let Some(f) = format(font, size, weight, align, ellipsis) else { return };
    let ws: Vec<u16> = s.encode_utf16().collect();
    let clip = if ellipsis { D2D1_DRAW_TEXT_OPTIONS_CLIP } else { D2D1_DRAW_TEXT_OPTIONS_NONE };
    rt.DrawText(&ws, &f, &rf(x, y, w, h), b, clip, DWRITE_MEASURING_MODE_NATURAL);
}

pub unsafe fn text_width(s: &str, font: &str, size: i32, weight: i32) -> f32 {
    let (Some(gx), Some(f)) = (g(), format(font, size, weight, 0, false)) else { return 0.0 };
    let ws: Vec<u16> = s.encode_utf16().collect();
    let Ok(layout) = gx.dw.CreateTextLayout(&ws, &f, 10000.0, 1000.0) else { return 0.0 };
    let mut m: DWRITE_TEXT_METRICS = std::mem::zeroed();
    if layout.GetMetrics(&mut m).is_err() {
        return 0.0;
    }
    m.widthIncludingTrailingWhitespace
}

unsafe fn push_layer(mask: Option<ID2D1Geometry>, opacity: f32) {
    let Some(gx) = g() else { return };
    if !gx.drawing {
        return;
    }
    let params = D2D1_LAYER_PARAMETERS {
        contentBounds: D2D_RECT_F { left: -1e6, top: -1e6, right: 1e6, bottom: 1e6 },
        geometricMask: std::mem::ManuallyDrop::new(mask),
        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
        maskTransform: Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 },
        opacity,
        opacityBrush: std::mem::ManuallyDrop::new(None),
        layerOptions: D2D1_LAYER_OPTIONS_NONE,
    };
    gx.rt.PushLayer(&params, None);
    let mut params = params;
    std::mem::ManuallyDrop::drop(&mut params.geometricMask);
    gx.layers.push(true);
}

/// Clip what follows to a rounded rectangle (anti-aliased edge).
pub unsafe fn push_round_clip(x: f32, y: f32, w: f32, h: f32, r: f32) {
    let Some(gx) = g() else { return };
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let rr = D2D1_ROUNDED_RECT { rect: rf(x, y, w, h), radiusX: r, radiusY: r };
    if let Ok(geo) = gx.factory.CreateRoundedRectangleGeometry(&rr) {
        push_layer(geo.cast::<ID2D1Geometry>().ok(), 1.0);
    }
}

pub unsafe fn push_opacity(a: f32) {
    push_layer(None, a.clamp(0.0, 1.0));
}

pub unsafe fn pop() {
    if let Some(gx) = g() {
        if let Some(is_layer) = gx.layers.pop() {
            if is_layer { gx.rt.PopLayer() } else { gx.rt.PopAxisAlignedClip() }
        }
    }
}

/// Draw a BGRA image (`size` x `size`), cached by `gen`.
pub unsafe fn image(gen: u64, size: i32, bgra: &[u8], x: f32, y: f32, w: f32, h: f32) {
    let Some(gx) = g() else { return };
    if !gx.drawing || size <= 0 || bgra.len() < (size * size * 4) as usize {
        return;
    }
    if gx.bitmap.as_ref().map(|b| b.0) != Some(gen) {
        gx.bitmap = None;
        let props = D2D1_BITMAP_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
            dpiX: 96.0,
            dpiY: 96.0,
        };
        if let Ok(b) = gx.rt.CreateBitmap(
            D2D_SIZE_U { width: size as u32, height: size as u32 },
            Some(bgra.as_ptr() as *const c_void),
            (size * 4) as u32,
            &props,
        ) {
            gx.bitmap = Some((gen, b));
        }
    }
    if let Some((_, b)) = &gx.bitmap {
        gx.rt.DrawBitmap(b, Some(&rf(x, y, w, h)), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None);
    }
}
