//! Echte macOS-Icons statt Ersatzzeichen — dieselbe Quelle wie in der
//! SwiftUI-Fassung:
//!  • Dateisymbole über `NSWorkspace` (IconProvider.swift), zwischengespeichert
//!    nach Endung, Pakete wie .app aber nach Pfad, damit sie ihr echtes Icon behalten;
//!  • Werkzeugleisten-Symbole als SF Symbols über `NSImage(systemSymbolName:)`.
//!
//! Beides wird einmal in eine egui-Textur gerendert und dann nur noch gezeichnet.

use objc2::rc::Retained;
use objc2::AnyThread as _;
use objc2_app_kit::{
    NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace, NSGraphicsContext, NSImage,
    NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use objc2_uniform_type_identifiers::UTType;
use std::collections::HashMap;

/// Gerastert wird mit 2× für Retina, angezeigt mit `ICON_POINTS`.
const RASTER: usize = 32;

pub struct IconCache {
    files: HashMap<String, egui::TextureHandle>,
    symbols: HashMap<String, egui::TextureHandle>,
}

impl IconCache {
    pub fn new() -> Self {
        IconCache {
            files: HashMap::new(),
            symbols: HashMap::new(),
        }
    }

    /// Zeichnet das Icon eines Eintrags in `rect`.
    pub fn draw_file_icon(
        &mut self,
        ui: &egui::Ui,
        path: &std::path::Path,
        is_dir: bool,
        rect: egui::Rect,
    ) {
        if let Some(id) = self.file_icon(ui.ctx(), path, is_dir) {
            ui.painter().image(id, rect, UV, egui::Color32::WHITE);
        }
    }

    /// Zeichnet ein SF Symbol, eingefärbt.
    pub fn draw_symbol(&mut self, ui: &egui::Ui, name: &str, rect: egui::Rect, color: egui::Color32) {
        if let Some(id) = self.symbol(ui.ctx(), name) {
            ui.painter().image(id, rect, UV, color);
        }
    }

    /// Das Icon für einen Eintrag, mit derselben Cache-Strategie wie IconProvider.
    fn file_icon(
        &mut self,
        ctx: &egui::Context,
        path: &std::path::Path,
        is_dir: bool,
    ) -> Option<egui::TextureId> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let key = if is_dir {
            if ext.is_empty() {
                "·folder".to_string()
            } else {
                // Ein Paket (.app, .bundle …) — echtes Icon, deshalb pro Pfad.
                format!("path:{}", path.display())
            }
        } else {
            format!("ext:{ext}")
        };

        if let Some(t) = self.files.get(&key) {
            return Some(t.id());
        }

        let image = unsafe { load_file_icon(path, is_dir, &ext) }?;
        let handle = ctx.load_texture(&key, image, texture_options());
        let id = handle.id();
        self.files.insert(key, handle);
        Some(id)
    }

    /// Ein SF Symbol als weiße Maske — die Farbe kommt beim Zeichnen per Tönung,
    /// damit dasselbe Symbol aktiv, inaktiv und deaktiviert benutzt werden kann.
    fn symbol(&mut self, ctx: &egui::Context, name: &str) -> Option<egui::TextureId> {
        if let Some(t) = self.symbols.get(name) {
            return Some(t.id());
        }
        let image = unsafe { load_symbol(name) }?;
        let handle = ctx.load_texture(name, image, texture_options());
        let id = handle.id();
        self.symbols.insert(name.to_string(), handle);
        Some(id)
    }
}

/// Volle Textur, kein Ausschnitt.
const UV: egui::Rect = egui::Rect {
    min: egui::Pos2 { x: 0.0, y: 0.0 },
    max: egui::Pos2 { x: 1.0, y: 1.0 },
};

fn texture_options() -> egui::TextureOptions {
    egui::TextureOptions {
        magnification: egui::TextureFilter::Linear,
        minification: egui::TextureFilter::Linear,
        ..Default::default()
    }
}

unsafe fn load_file_icon(
    path: &std::path::Path,
    is_dir: bool,
    ext: &str,
) -> Option<egui::ColorImage> {
    let workspace = NSWorkspace::sharedWorkspace();

    let image: Retained<NSImage> = if is_dir {
        if ext.is_empty() {
            // Der generische Ordner, nicht der eines konkreten Pfads: sonst
            // würde ein Ordner mit eigenem Icon den Cache für alle vergiften.
            let ut = UTType::typeWithIdentifier(&NSString::from_str("public.folder"))?;
            workspace.iconForContentType(&ut)
        } else {
            workspace.iconForFile(&NSString::from_str(&path.to_string_lossy()))
        }
    } else if !ext.is_empty() {
        match UTType::typeWithFilenameExtension(&NSString::from_str(ext)) {
            Some(ut) => workspace.iconForContentType(&ut),
            None => generic_data_icon(&workspace)?,
        }
    } else {
        generic_data_icon(&workspace)?
    };

    rasterize(&image, false)
}

unsafe fn generic_data_icon(workspace: &NSWorkspace) -> Option<Retained<NSImage>> {
    let ut = UTType::typeWithIdentifier(&NSString::from_str("public.data"))?;
    Some(workspace.iconForContentType(&ut))
}

unsafe fn load_symbol(name: &str) -> Option<egui::ColorImage> {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        None,
    )?;
    // Als weiße Maske rastern, damit egui sie beliebig einfärben kann.
    rasterize(&image, true)
}

/// Zeichnet ein NSImage in einen RGBA-Puffer und macht daraus ein egui-Bild.
unsafe fn rasterize(image: &NSImage, as_white_mask: bool) -> Option<egui::ColorImage> {
    let rep = NSBitmapImageRep::alloc();
    let rep = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
        rep,
        std::ptr::null_mut(),
        RASTER as isize,
        RASTER as isize,
        8,
        4,
        true,
        false,
        NSDeviceRGBColorSpace,
        (RASTER * 4) as isize,
        32,
    )?;

    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&context));

    let rect = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(RASTER as f64, RASTER as f64),
    );
    image.drawInRect_fromRect_operation_fraction(
        rect,
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
        NSCompositingOperation::SourceOver,
        1.0,
    );

    NSGraphicsContext::restoreGraphicsState_class();

    let data = rep.bitmapData();
    if data.is_null() {
        return None;
    }
    let bytes_per_row = rep.bytesPerRow() as usize;
    let mut pixels = Vec::with_capacity(RASTER * RASTER);
    for y in 0..RASTER {
        let row = data.add(y * bytes_per_row);
        for x in 0..RASTER {
            let p = row.add(x * 4);
            let (r, g, b, a) = (*p, *p.add(1), *p.add(2), *p.add(3));
            // NSBitmapImageRep liefert vormultipliziertes Alpha — genau das,
            // was egui bei from_rgba_premultiplied erwartet.
            pixels.push(if as_white_mask {
                // Weiß mit demselben Alpha: dann ergibt Tönung × Weiß die Tönung.
                egui::Color32::from_rgba_premultiplied(a, a, a, a)
            } else {
                egui::Color32::from_rgba_premultiplied(r, g, b, a)
            });
        }
    }

    Some(egui::ColorImage::new([RASTER, RASTER], pixels))
}

// MARK: - Schriften und Akzentfarbe

/// Lädt die macOS-Systemschriften in egui. Ohne das nimmt egui Ubuntu-Light,
/// und die Kürzelzeichen (⌘⇧⏎⌫) fehlen komplett — sie stehen in Apple Symbols.
pub fn install_fonts(ctx: &egui::Context) {
    use std::sync::Arc;

    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, path: &str, family: egui::FontFamily, front: bool| {
        let Ok(bytes) = std::fs::read(path) else { return };
        fonts
            .font_data
            .insert(name.to_string(), Arc::new(egui::FontData::from_owned(bytes)));
        let list = fonts.families.entry(family).or_default();
        if front {
            list.insert(0, name.to_string());
        } else {
            list.push(name.to_string());
        }
    };

    // SF Pro als Fließtext — dieselbe Schrift, die SwiftUI benutzt.
    add(
        "sf-pro",
        "/System/Library/Fonts/SFNS.ttf",
        egui::FontFamily::Proportional,
        true,
    );
    // SF Mono für Pfadleiste, Datum und Größe (dort mit Ziffern gleicher Breite).
    add(
        "sf-mono",
        "/System/Library/Fonts/SFNSMono.ttf",
        egui::FontFamily::Monospace,
        true,
    );
    // Apple Symbols als Rückfall für ⌘ ⇧ ⌥ ⌫ ⏎ ⌃ — in beiden Familien.
    add(
        "apple-symbols",
        "/System/Library/Fonts/Apple Symbols.ttf",
        egui::FontFamily::Proportional,
        false,
    );
    add(
        "apple-symbols",
        "/System/Library/Fonts/Apple Symbols.ttf",
        egui::FontFamily::Monospace,
        false,
    );

    ctx.set_fonts(fonts);
}

/// Die vom Nutzer in den Systemeinstellungen gewählte Akzentfarbe — dieselbe
/// Quelle wie `Color.accentColor` in SwiftUI.
pub fn system_accent() -> egui::Color32 {
    let color = objc2_app_kit::NSColor::controlAccentColor();
    // In den Gerätefarbraum wandeln, sonst wirft der Zugriff auf die einzelnen
    // Komponenten bei Katalogfarben eine Ausnahme.
    let Some(rgb) =
        color.colorUsingColorSpace(&objc2_app_kit::NSColorSpace::deviceRGBColorSpace())
    else {
        return egui::Color32::from_rgb(0, 122, 255); // macOS-Blau
    };
    egui::Color32::from_rgb(
        (rgb.redComponent() * 255.0).round() as u8,
        (rgb.greenComponent() * 255.0).round() as u8,
        (rgb.blueComponent() * 255.0).round() as u8,
    )
}
