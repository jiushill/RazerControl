//! 官方产品图渲染：解码 → 白底抠图 → 生成 GPU 纹理 → 按裁剪框等比绘制。
//!
//! 素材为 Razer 官方产品图（DeathAdder V3 Pro 顶视），仅本地个人使用。
//! 抠图用「从四边泛洪」而不是全局阈值，避免把机身内部的高光一起打穿。

use egui::{
    Color32, ColorImage, Context, Pos2, Rect, Rounding, Stroke, TextureHandle, TextureOptions, Vec2,
};

/// DeathAdder V3 Pro 素材的全图归一化裁剪框：[x0, y0, x1, y1]
pub const CROP_DAV3PRO: [f32; 4] = [0.250, 0.055, 0.752, 0.910];

/// DeathAdder V3 Pro 的 5 个按键中心点（左键 / 右键 / 中键 / 侧键·前 / 侧键·后）
///
/// 侧键坐标是在 `device_dav3pro.png`（500×500）上逐像素量出来的：
/// 按键面 x = 152..171，上键 y = 180..222、下键 y = 232..290，
/// 取中心 (161, 201) 与 (161, 261) 再除以 500。
pub const HOTSPOTS_DAV3PRO: [[f32; 2]; 5] = [
    [0.394, 0.258],
    [0.606, 0.258],
    [0.494, 0.302],
    [0.338, 0.400],
    [0.338, 0.520],
];

/// DeathAdder V2 Pro 素材（750×420 顶视渲染）的裁剪框
pub const CROP_DAV2PRO: [f32; 4] = [0.355, 0.110, 0.650, 0.918];

/// DeathAdder V2 Pro 的 5 个按键中心点（左键 / 右键 / 中键 / 后退 / 前进）
pub const HOTSPOTS_DAV2PRO: [[f32; 2]; 5] = [
    [0.447, 0.226],
    [0.560, 0.214],
    [0.495, 0.238],
    [0.408, 0.387],
    [0.411, 0.452],
];

/// 索引 3/4 对应几何上的靠前/靠后侧键；实测该型号靠前侧键发 XBUTTON2(=前进)
pub const BUTTON_NAMES: [&str; 5] = ["左键", "右键", "中键", "前进", "后退"];

pub struct Photo {
    pub tex: TextureHandle,
    /// 裁剪区域的宽高比
    pub aspect: f32,
    pub crop: [f32; 4],
    pub hotspots: [[f32; 2]; 5],
}

fn is_bg(r: u8, g: u8, b: u8) -> bool {
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    mx >= 248 && (mx - mn) <= 6
}

/// 从四边泛洪去白底，并对边缘做一次羽化。
fn key_out_background(px: &mut [u8], w: usize, h: usize) {
    let mut visited = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::with_capacity(w * 2 + h * 2);
    let mut push = |x: usize, y: usize, stack: &mut Vec<usize>, visited: &mut Vec<bool>| {
        let i = y * w + x;
        if !visited[i] {
            let o = i * 4;
            if is_bg(px[o], px[o + 1], px[o + 2]) {
                visited[i] = true;
                stack.push(i);
            } else {
                visited[i] = true;
            }
        }
    };
    for x in 0..w {
        push(x, 0, &mut stack, &mut visited);
        push(x, h - 1, &mut stack, &mut visited);
    }
    for y in 0..h {
        push(0, y, &mut stack, &mut visited);
        push(w - 1, y, &mut stack, &mut visited);
    }

    while let Some(i) = stack.pop() {
        px[i * 4 + 3] = 0;
        let x = i % w;
        let y = i / w;
        let mut visit = |nx: usize, ny: usize, stack: &mut Vec<usize>, visited: &mut Vec<bool>| {
            let j = ny * w + nx;
            if !visited[j] {
                visited[j] = true;
                let o = j * 4;
                if is_bg(px[o], px[o + 1], px[o + 2]) {
                    stack.push(j);
                }
            }
        };
        if x > 0 {
            visit(x - 1, y, &mut stack, &mut visited);
        }
        if x + 1 < w {
            visit(x + 1, y, &mut stack, &mut visited);
        }
        if y > 0 {
            visit(x, y - 1, &mut stack, &mut visited);
        }
        if y + 1 < h {
            visit(x, y + 1, &mut stack, &mut visited);
        }
    }

    // 羽化：贴着透明区的浅色像素降半透明，去掉锯齿白边
    let src = px.to_vec();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if src[i * 4 + 3] == 0 {
                continue;
            }
            let mut edge = false;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    if src[((ny as usize) * w + nx as usize) * 4 + 3] == 0 {
                        edge = true;
                    }
                }
            }
            if edge {
                let o = i * 4;
                let bright = src[o] as u16 + src[o + 1] as u16 + src[o + 2] as u16;
                if bright > 540 {
                    px[o + 3] = 90;
                } else if bright > 420 {
                    px[o + 3] = 170;
                }
            }
        }
    }
}

/// 是否有该 PID 对应的官方图素材
pub fn has_photo_for(pid: u16) -> bool {
    matches!(pid, 0x00B6 | 0x00B7 | 0x00B8 | 0x00B9 | 0x007C | 0x007D)
}

pub fn load(
    ctx: &Context,
    bytes: &[u8],
    key: &str,
    crop: [f32; 4],
    hotspots: [[f32; 2]; 5],
) -> Option<Photo> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w0, h0) = img.dimensions();
    let img = if w0 > 700 || h0 > 700 {
        let scale = 700.0 / w0.max(h0) as f32;
        image::imageops::resize(
            &img,
            (w0 as f32 * scale) as u32,
            (h0 as f32 * scale) as u32,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    let (w, h) = img.dimensions();
    let mut px = img.into_raw();
    key_out_background(&mut px, w as usize, h as usize);
    // 提亮暗部：黑色机身在深色主题下会"糊"进背景
    for c in px.chunks_mut(4) {
        if c[3] == 0 {
            continue;
        }
        for k in 0..3 {
            let v = c[k] as f32 / 255.0;
            c[k] = (255.0 * v.powf(0.80)).round().clamp(0.0, 255.0) as u8;
        }
    }
    let color = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
    let tex = ctx.load_texture(key, color, TextureOptions::LINEAR);
    let aspect = ((crop[2] - crop[0]) * w as f32) / ((crop[3] - crop[1]) * h as f32);
    Some(Photo {
        tex,
        aspect,
        crop,
        hotspots,
    })
}

fn to_screen(rect: Rect, p: [f32; 2], crop: [f32; 4]) -> Pos2 {
    let fx = (p[0] - crop[0]) / (crop[2] - crop[0]);
    let fy = (p[1] - crop[1]) / (crop[3] - crop[1]);
    Pos2::new(
        rect.left() + fx * rect.width(),
        rect.top() + fy * rect.height(),
    )
}

/// 在 `area` 内等比绘制产品图，叠加编号热点标记，返回 5 个点击热区。
pub fn draw(
    painter: &egui::Painter,
    area: Rect,
    photo: &Photo,
    sel: Option<usize>,
    hover: Option<usize>,
) -> [Rect; 5] {
    let width = (area.height() * photo.aspect).min(area.width());
    let height = width / photo.aspect;
    let rect = Rect::from_center_size(area.center(), Vec2::new(width, height));

    let uv = Rect::from_min_max(
        Pos2::new(photo.crop[0], photo.crop[1]),
        Pos2::new(photo.crop[2], photo.crop[3]),
    );

    // 投影（同一纹理按 alpha 叠加，形成贴合轮廓的柔和阴影）
    for (dy, a) in [(11.0f32, 34u8), (7.0, 62), (3.5, 88)] {
        painter.image(
            photo.tex.id(),
            rect.translate(Vec2::new(0.0, dy)),
            uv,
            Color32::from_rgba_unmultiplied(0, 0, 0, a),
        );
    }
    painter.image(photo.tex.id(), rect, uv, Color32::WHITE);

    let semi = egui::FontFamily::Name("semi".into());
    let mut hits = [Rect::NOTHING; 5];
    for (i, c) in photo.hotspots.iter().enumerate() {
        let center = to_screen(rect, *c, photo.crop);
        let hit_size = if i >= 3 { 28.0 } else { 34.0 };
        let hit = Rect::from_center_size(center, Vec2::splat(hit_size));
        hits[i] = hit;
        let active = sel == Some(i);
        let hovered = hover == Some(i);
        // 侧键（4/5）本身很窄，圆点用大尺寸会盖到机身外，这里按比例缩小
        let k = if i >= 3 { 0.72 } else { 1.0 };
        let r = if active {
            9.0
        } else if hovered {
            8.0
        } else {
            7.2
        } * k;
        if active || hovered {
            painter.circle_filled(
                center,
                r + 7.0 * k,
                crate::theme::alpha(crate::theme::ACCENT, if active { 46 } else { 22 }),
            );
        }
        painter.circle_filled(
            center,
            r,
            if active {
                crate::theme::ACCENT
            } else {
                Color32::from_rgb(0x11, 0x17, 0x14)
            },
        );
        painter.circle_stroke(
            center,
            r,
            Stroke::new(
                1.2,
                if active {
                    crate::theme::ACCENT
                } else {
                    crate::theme::mix(crate::theme::BORDER_HI, crate::theme::TEXT, 0.25)
                },
            ),
        );
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            (i + 1).to_string(),
            egui::FontId::new(10.0, semi.clone()),
            if active {
                crate::theme::ACCENT_DARK
            } else {
                crate::theme::DIM
            },
        );
    }

    hits
}
