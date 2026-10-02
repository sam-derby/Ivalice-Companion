#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Resource,
    Unsupported,
    Corrupt,
    Limit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainerError {
    InputEmpty,
    InputLimit,
    PngSignature,
    PngChunkLimit,
    PngChunkTruncated,
    PngChunkLength,
    PngChunkType,
    PngReservedBit,
    PngChunkChecksum,
    PngHeader,
    PngPalette,
    PngImageDataOrder,
    PngUnknownCritical,
    PngEnd,
    PngTrailing,
    PngFftoCount,
    PngFftoEmpty,
    ResourceMissing,
    ResourceLength,
    ResourceMismatch,
    UmifHeaderTruncated,
    UmifHeaderSize,
    UmifReservedWord,
    UmifMagic,
    UmifFileCount,
    UmifNameLength,
    UmifNegativeValue,
    UmifRange,
    UmifNameOffset,
    UmifDataOffset,
    UmifDataEnd,
    UmifPadding,
    UmifFilename,
    DictionaryId,
    CodecDictionary,
    Codec,
    OutputLimit,
    StreamIncomplete,
    StreamTrailing,
    DecodedLength,
    PayloadTruncated,
    PayloadChecksum,
    UnsupportedVersion,
    UnsupportedDiscriminator,
    UnsupportedLength,
}

impl ContainerError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InputEmpty => "input_empty",
            Self::InputLimit => "input_limit",
            Self::PngSignature => "png_signature",
            Self::PngChunkLimit => "png_chunk_limit",
            Self::PngChunkTruncated => "png_chunk_truncated",
            Self::PngChunkLength => "png_chunk_length",
            Self::PngChunkType => "png_chunk_type",
            Self::PngReservedBit => "png_reserved_bit",
            Self::PngChunkChecksum => "png_chunk_checksum",
            Self::PngHeader => "png_header",
            Self::PngPalette => "png_palette",
            Self::PngImageDataOrder => "png_image_data_order",
            Self::PngUnknownCritical => "png_unknown_critical",
            Self::PngEnd => "png_end",
            Self::PngTrailing => "png_trailing",
            Self::PngFftoCount => "png_ffto_count",
            Self::PngFftoEmpty => "png_ffto_empty",
            Self::ResourceMissing => "resource_missing",
            Self::ResourceLength => "resource_length",
            Self::ResourceMismatch => "resource_mismatch",
            Self::UmifHeaderTruncated => "umif_header_truncated",
            Self::UmifHeaderSize => "umif_header_size",
            Self::UmifReservedWord => "umif_reserved_word",
            Self::UmifMagic => "umif_magic",
            Self::UmifFileCount => "umif_file_count",
            Self::UmifNameLength => "umif_name_length",
            Self::UmifNegativeValue => "umif_negative_value",
            Self::UmifRange => "umif_range",
            Self::UmifNameOffset => "umif_name_offset",
            Self::UmifDataOffset => "umif_data_offset",
            Self::UmifDataEnd => "umif_data_end",
            Self::UmifPadding => "umif_padding",
            Self::UmifFilename => "umif_filename",
            Self::DictionaryId => "dictionary_id",
            Self::CodecDictionary => "codec_dictionary",
            Self::Codec => "codec",
            Self::OutputLimit => "output_limit",
            Self::StreamIncomplete => "stream_incomplete",
            Self::StreamTrailing => "stream_trailing",
            Self::DecodedLength => "decoded_length",
            Self::PayloadTruncated => "payload_truncated",
            Self::PayloadChecksum => "payload_checksum",
            Self::UnsupportedVersion => "unsupported_version",
            Self::UnsupportedDiscriminator => "unsupported_discriminator",
            Self::UnsupportedLength => "unsupported_length",
        }
    }

    #[must_use]
    pub const fn kind(self) -> ErrorKind {
        match self {
            Self::ResourceMissing | Self::ResourceLength | Self::ResourceMismatch => {
                ErrorKind::Resource
            }
            Self::UnsupportedVersion | Self::UnsupportedDiscriminator | Self::UnsupportedLength => {
                ErrorKind::Unsupported
            }
            Self::InputLimit | Self::PngChunkLimit | Self::OutputLimit => ErrorKind::Limit,
            _ => ErrorKind::Corrupt,
        }
    }
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ContainerError {}
