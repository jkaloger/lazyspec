use ratatui_image::picker::{Picker, ProtocolType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalImageProtocol {
    Sixel,
    KittyGraphics,
    Iterm2,
    Halfblocks,
    Unsupported,
}

impl TerminalImageProtocol {
    pub fn supports_images(self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

impl From<ProtocolType> for TerminalImageProtocol {
    fn from(pt: ProtocolType) -> Self {
        match pt {
            ProtocolType::Kitty => Self::KittyGraphics,
            ProtocolType::Sixel => Self::Sixel,
            ProtocolType::Iterm2 => Self::Iterm2,
            ProtocolType::Halfblocks => Self::Halfblocks,
        }
    }
}

pub fn create_picker() -> Picker {
    Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks())
}
