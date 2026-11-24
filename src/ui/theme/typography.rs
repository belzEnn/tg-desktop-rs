use iced::{Font, font::Weight, widget::text::LineHeight};

pub(crate) const BODY_FONT: Font = Font::DEFAULT;
pub(crate) const CODE_FONT: Font = Font::MONOSPACE;
pub(crate) const CAPTION: u32 = 12;
pub(crate) const PREVIEW: u32 = 13;
pub(crate) const DIALOG_TITLE: u32 = 18;
pub(crate) const CHAT_TITLE: u32 = 20;
pub(crate) const HEADING_FONT: Font = Font {
    weight: Weight::Semibold,
    ..BODY_FONT
};
pub(crate) const BODY_SIZE: u32 = 15;
pub(crate) const ROW_TITLE: u32 = 14;
pub(crate) const MESSAGE_LINE_HEIGHT: LineHeight = LineHeight::Relative(1.32);
