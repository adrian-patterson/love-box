//! Bundled SIL OFL font faces, compiled into the binary with `include_bytes!`.
//!
//! Files and licences live in `lovebox/fonts/<Family>/` (each with its `OFL.txt`), taken
//! from the google/fonts repository. EB Garamond, Libre Baskerville and Lora are published
//! there only as variable `[wght]` fonts; the renderer draws their default instance, which
//! is `wght = 400` (Regular) in all three files.

use ab_glyph::FontRef;

/// Id of the default face for quotes and messages: EB Garamond, the classic serif.
pub const DEFAULT_FONT: &str = "eb-garamond";

/// One bundled face.
#[derive(Clone, Debug)]
pub struct Face {
    id: &'static str,
    display_name: &'static str,
    family: &'static str,
    italic: bool,
    data: &'static [u8],
    font: FontRef<'static>,
}

impl Face {
    /// Stable id for configs and the API, e.g. `"eb-garamond-italic"`.
    pub fn id(&self) -> &'static str {
        self.id
    }

    /// Human-readable name, e.g. `"EB Garamond Italic"`.
    pub fn display_name(&self) -> &'static str {
        self.display_name
    }

    /// Family name, e.g. `"EB Garamond"`.
    pub fn family(&self) -> &'static str {
        self.family
    }

    /// Whether this is an italic face.
    pub fn is_italic(&self) -> bool {
        self.italic
    }

    /// Size of the embedded font file in bytes.
    pub fn file_size(&self) -> usize {
        self.data.len()
    }

    /// The bundled TrueType bytes, for serving the same face to the local web UI.
    pub fn data(&self) -> &'static [u8] {
        self.data
    }

    /// The parsed font, for the renderer.
    pub fn font(&self) -> &FontRef<'static> {
        &self.font
    }
}

/// (id, display name, family, italic, file bytes).
type Entry = (
    &'static str,
    &'static str,
    &'static str,
    bool,
    &'static [u8],
);

const BUNDLED: &[Entry] = &[
    (
        "eb-garamond",
        "EB Garamond",
        "EB Garamond",
        false,
        include_bytes!("../fonts/EBGaramond/EBGaramond[wght].ttf"),
    ),
    (
        "eb-garamond-italic",
        "EB Garamond Italic",
        "EB Garamond",
        true,
        include_bytes!("../fonts/EBGaramond/EBGaramond-Italic[wght].ttf"),
    ),
    (
        "libre-baskerville",
        "Libre Baskerville",
        "Libre Baskerville",
        false,
        include_bytes!("../fonts/LibreBaskerville/LibreBaskerville[wght].ttf"),
    ),
    (
        "libre-baskerville-italic",
        "Libre Baskerville Italic",
        "Libre Baskerville",
        true,
        include_bytes!("../fonts/LibreBaskerville/LibreBaskerville-Italic[wght].ttf"),
    ),
    (
        "lora",
        "Lora",
        "Lora",
        false,
        include_bytes!("../fonts/Lora/Lora[wght].ttf"),
    ),
    (
        "lora-italic",
        "Lora Italic",
        "Lora",
        true,
        include_bytes!("../fonts/Lora/Lora-Italic[wght].ttf"),
    ),
    (
        "atkinson-hyperlegible",
        "Atkinson Hyperlegible",
        "Atkinson Hyperlegible",
        false,
        include_bytes!("../fonts/AtkinsonHyperlegible/AtkinsonHyperlegible-Regular.ttf"),
    ),
];

/// The set of faces the renderer can use. Cheap to build (fonts are parsed lazily).
#[derive(Clone, Debug)]
pub struct Fonts {
    faces: Vec<Face>,
}

/// Lower-case alphanumerics only, so `"EB Garamond Italic"`, `"eb-garamond-italic"` and
/// `"eb_garamond_italic"` all name the same face.
fn normalise(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

impl Fonts {
    /// The faces compiled into the binary.
    pub fn bundled() -> Self {
        let faces = BUNDLED
            .iter()
            .map(|&(id, display_name, family, italic, data)| Face {
                id,
                display_name,
                family,
                italic,
                data,
                // The bundled files are fixed at compile time and checked by tests.
                font: FontRef::try_from_slice(data).expect("bundled font parses"),
            })
            .collect();
        Self { faces }
    }

    /// Looks a face up by id or display name, ignoring case, spaces, `-` and `_`.
    pub fn by_name(&self, name: &str) -> Option<&Face> {
        let wanted = normalise(name);
        self.faces
            .iter()
            .find(|f| normalise(f.id) == wanted || normalise(f.display_name) == wanted)
    }

    /// Ids of all faces, in a stable order (the default first).
    pub fn names(&self) -> Vec<&'static str> {
        self.faces.iter().map(|f| f.id).collect()
    }

    /// All faces.
    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    /// The default face ([`DEFAULT_FONT`]).
    pub fn default_face(&self) -> &Face {
        self.by_name(DEFAULT_FONT).expect("default font is bundled")
    }

    /// Total bytes of embedded font files.
    pub fn embedded_bytes(&self) -> usize {
        self.faces.iter().map(|f| f.data.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_glyph::Font;

    #[test]
    fn bundled_faces_parse_and_have_latin_glyphs() {
        let fonts = Fonts::bundled();
        assert_eq!(fonts.faces().len(), 7);
        for face in fonts.faces() {
            for c in "AaZz09&—éß“”".chars() {
                assert_ne!(face.font().glyph_id(c).0, 0, "{} lacks {c:?}", face.id());
            }
        }
        assert_eq!(fonts.names()[0], DEFAULT_FONT);
    }

    #[test]
    fn lookup_ignores_case_and_separators() {
        let fonts = Fonts::bundled();
        assert_eq!(fonts.by_name("EB Garamond").unwrap().id(), "eb-garamond");
        assert_eq!(
            fonts.by_name("eb_garamond_ITALIC").unwrap().id(),
            "eb-garamond-italic"
        );
        assert_eq!(fonts.by_name("Lora Italic").unwrap().family(), "Lora");
        assert!(fonts.by_name("Atkinson Hyperlegible").is_some());
        assert!(fonts.by_name("Palatino").is_none());
        assert_eq!(fonts.default_face().id(), "eb-garamond");
    }

    #[test]
    fn families_cover_classic_modern_serif_and_sans() {
        let fonts = Fonts::bundled();
        let families: Vec<_> = fonts.faces().iter().map(|f| f.family()).collect();
        for family in [
            "EB Garamond",
            "Libre Baskerville",
            "Lora",
            "Atkinson Hyperlegible",
        ] {
            assert!(families.contains(&family), "{family} missing");
        }
        assert!(fonts.embedded_bytes() > 2_000_000);
    }
}
