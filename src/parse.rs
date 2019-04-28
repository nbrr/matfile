use libflate::zlib::Decoder;
use nom::{
    alt, apply, be_f32, be_f64, be_i16, be_i32, be_i64, be_i8, be_u16, be_u32, be_u64, be_u8, char,
    complete, cond, count, count_fixed, do_parse, error_position, i32, le_f32, le_f64, le_i16,
    le_i32, le_i64, le_i8, le_u16, le_u32, le_u64, le_u8, length_value, many0, map, map_res, not,
    opt, pair, peek, switch, tag, take, take_till, u16, u32, value, IResult,
};
use num_traits::FromPrimitive;
use std::io::Read;

// https://www.mathworks.com/help/pdf_doc/matlab/matfile_format.pdf
// https://www.mathworks.com/help/matlab/import_export/mat-file-versions.html

#[derive(Clone, Debug)]
pub struct Header {
    text: String,
    is_little_endian: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NumericData {
    Int8(Vec<i8>),
    UInt8(Vec<u8>),
    Int16(Vec<i16>),
    UInt16(Vec<u16>),
    Int32(Vec<i32>),
    UInt32(Vec<u32>),
    Int64(Vec<i64>),
    UInt64(Vec<u64>),
    Single(Vec<f32>),
    Double(Vec<f64>),
}

impl NumericData {
    fn len(&self) -> usize {
        match self {
            NumericData::Single(vec) => vec.len(),
            NumericData::Double(vec) => vec.len(),
            NumericData::Int8(vec) => vec.len(),
            NumericData::UInt8(vec) => vec.len(),
            NumericData::Int16(vec) => vec.len(),
            NumericData::UInt16(vec) => vec.len(),
            NumericData::Int32(vec) => vec.len(),
            NumericData::UInt32(vec) => vec.len(),
            NumericData::Int64(vec) => vec.len(),
            NumericData::UInt64(vec) => vec.len(),
        }
    }

    fn data_type(&self) -> DataType {
        match self {
            NumericData::Single(_) => DataType::Single,
            NumericData::Double(_) => DataType::Double,
            NumericData::Int8(_) => DataType::Int8,
            NumericData::UInt8(_) => DataType::UInt8,
            NumericData::Int16(_) => DataType::Int16,
            NumericData::UInt16(_) => DataType::UInt16,
            NumericData::Int32(_) => DataType::Int32,
            NumericData::UInt32(_) => DataType::UInt32,
            NumericData::Int64(_) => DataType::Int64,
            NumericData::UInt64(_) => DataType::UInt64,
        }
    }
}

#[derive(Clone, Debug)]
pub enum DataElement {
    // Cell Matrix,
    StructureMatrix(
        ArrayFlags,
        Dimensions,
        String,
        FieldNameLength,
        Vec<FieldName>,
        Fields,
    ),
    // Object Matrix,
    // CharacterMatrix,
    SparseMatrix(
        ArrayFlags,
        Dimensions,
        String,
        RowIndex,
        ColumnShift,
        NumericData,
        Option<NumericData>,
    ),
    NumericMatrix(
        ArrayFlags,
        Dimensions,
        String,
        NumericData,
        Option<NumericData>,
    ),
    Unsupported,
}

// #[cfg(feature = "ndarray")]
// {
//     #[derive(Debug)]
//     enum NumericArrayData {
//         Double(ndarray::ArrayD<f64>),
//     }

//     impl From<NumericData> for NumericArrayData {
//         fn from(nd: NumericData) -> Self;
//     }
// }

fn assert(i: &[u8], v: bool) -> IResult<&[u8], ()> {
    if v {
        Ok((i, ()))
    } else {
        Err(nom::Err::Failure(error_position!(
            i,
            nom::ErrorKind::Custom(48)
        )))
    }
}

pub fn parse_header(i: &[u8]) -> IResult<&[u8], Header> {
    do_parse!(
        i,
        // Make sure that the first four bytes are not null.
        peek!(count_fixed!(_, pair!(not!(char!('\0')), take!(1)), 4)) >>
        text: take!(116) >> // text field
        _ssdo: take!(8) >> // subsystem data offset
        // Assume little endian for now
        version: u16!(nom::Endianness::Little) >>
        // Check the endianness
        is_little_endian: alt!(value!(true, tag!("IM")) | value!(false, tag!("MI"))) >>
        // Fix endianness of the version field if we assumed the wrong one
        version: value!(if is_little_endian { version } else { version.swap_bytes() }) >>
        apply!(assert, version == 0x0100) >>
        (Header {
            text: std::str::from_utf8(text).unwrap_or(&"").to_owned(),
            is_little_endian: is_little_endian,
        })
    )
}

fn parse_next_data_element(i: &[u8], endianness: nom::Endianness) -> IResult<&[u8], DataElement> {
    do_parse!(
        i,
        data_element_tag: apply!(parse_data_element_tag, endianness) >>
        next_parser: value!(
            match data_element_tag.data_type {
                DataType::Matrix => parse_matrix_data_element,
                DataType::Compressed => parse_compressed_data_element,
                _ => {
                    println!("Unsupported variable type: {:?} (must be Matrix or Compressed)", data_element_tag.data_type);
                    parse_unsupported_data_element
                }
            }
        ) >>
        data_element: length_value!(value!(data_element_tag.data_byte_size), apply!(next_parser, endianness)) >>
        // Take care of padding. It seems like either all variables in a mat file compressed or none are.
        // If the variables are compressed there is no alignment to take care of (only uncompressed data
        // needs to be aligned according to the spec). Otherwise make sure that we end up on a 8 byte
        // boundary (ignore if there is not enough data left)
        padding_bytes: value!(if data_element_tag.data_type == DataType::Compressed { 0 } else { data_element_tag.padding_byte_size }) >>
        opt!(complete!(take!(padding_bytes))) >>
        (data_element)
    )
}

fn ceil_to_multiple(x: u32, multiple: u32) -> u32 {
    if x > 0 {
        (((x - 1) / multiple) + 1) * multiple
    } else {
        0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ArrayFlags {
    pub complex: bool,
    pub global: bool,
    pub logical: bool,
    pub class: ArrayType,
    pub nzmax: usize,
}

#[derive(Debug, PartialEq, Clone, Copy, Primitive)]
pub enum DataType {
    Int8 = 1,
    UInt8 = 2,
    Int16 = 3,
    UInt16 = 4,
    Int32 = 5,
    UInt32 = 6,
    Single = 7,
    Double = 9,
    Int64 = 12,
    UInt64 = 13,
    Matrix = 14,
    Compressed = 15,
    Utf8 = 16,
    Utf16 = 17,
    Utf32 = 18,
}

// impl DataType {
//     fn byte_size(&self) -> Option<usize> {
//         match self {
//             DataType::Int8 | DataType::UInt8 | DataType::Utf8 => Some(1),
//             DataType::Int16 | DataType::UInt16 | DataType::Utf16 => Some(2),
//             DataType::Int32 | DataType::UInt32 | DataType::Single | DataType::Utf32 => Some(4),
//             DataType::Int64 | DataType::UInt64 | DataType::Double => Some(8),
//             _ => None,
//         }
//     }
// }

#[derive(Debug, PartialEq, Clone, Copy, Primitive)]
pub enum ArrayType {
    Cell = 1,
    Struct = 2,
    Object = 3,
    Char = 4,
    Sparse = 5,
    Double = 6,
    Single = 7,
    Int8 = 8,
    UInt8 = 9,
    Int16 = 10,
    UInt16 = 11,
    Int32 = 12,
    UInt32 = 13,
    Int64 = 14,
    UInt64 = 15,
}

impl ArrayType {
    // fn is_numeric(&self) -> bool {
    //     match self {
    //         ArrayType::Cell
    //         | ArrayType::Struct
    //         | ArrayType::Object
    //         | ArrayType::Char
    //         | ArrayType::Sparse => false,
    //         _ => true,
    //     }
    // }

    fn numeric_data_type(&self) -> Option<DataType> {
        match self {
            ArrayType::Double => Some(DataType::Double),
            ArrayType::Single => Some(DataType::Single),
            ArrayType::Int8 => Some(DataType::Int8),
            ArrayType::UInt8 => Some(DataType::UInt8),
            ArrayType::Int16 => Some(DataType::Int16),
            ArrayType::UInt16 => Some(DataType::UInt16),
            ArrayType::Int32 => Some(DataType::UInt32),
            ArrayType::UInt32 => Some(DataType::UInt32),
            ArrayType::Int64 => Some(DataType::Int64),
            ArrayType::UInt64 => Some(DataType::UInt64),
            _ => None,
        }
    }
}

pub type Dimensions = Vec<i32>;

#[derive(Clone, Copy, Debug)]
pub struct DataElementTag {
    data_type: DataType,
    data_byte_size: u32,
    padding_byte_size: u32,
}

fn parse_data_element_tag(i: &[u8], endianness: nom::Endianness) -> IResult<&[u8], DataElementTag> {
    switch!(
        i,
        map!(peek!(u32!(endianness)), |b| b & 0xFFFF0000),
        0 => do_parse!(
            // Long Data Element Format
            data_type: u32!(endianness) >>
            byte_size: u32!(endianness) >>
            (DataElementTag {
                data_type: DataType::from_u32(data_type).ok_or(nom::Err::Failure(nom::Context::Code(
                    i,
                    nom::ErrorKind::Custom(46)
                )))?,
                data_byte_size: byte_size,
                padding_byte_size: ceil_to_multiple(byte_size, 8) - byte_size,
            })
        ) |
        _ => do_parse!(
            // Small Data Element Format
            data_type: map!(peek!(u32!(endianness)), |b| b & 0x0000FFFF) >>
            byte_size: map!(u32!(endianness), |b| (b & 0xFFFF0000) >> 16) >>
            (DataElementTag {
                data_type: DataType::from_u32(data_type).ok_or(nom::Err::Failure(nom::Context::Code(
                    i,
                    nom::ErrorKind::Custom(46)
                )))?,
                // TODO: assert that byte_size is <= 4
                data_byte_size: byte_size as u32,
                padding_byte_size: 4 - byte_size as u32,
            })
        )
    )
}

fn parse_array_name_subelement(i: &[u8], endianness: nom::Endianness) -> IResult<&[u8], String> {
    do_parse!(
        i,
        data_element_tag: apply!(parse_data_element_tag, endianness)
            >> apply!(
                assert,
                data_element_tag.data_type == DataType::Int8 && data_element_tag.data_byte_size > 0
            )
            >> name: map_res!(take!(data_element_tag.data_byte_size), |b| {
                std::str::from_utf8(b)
                    .map(|s| s.to_owned())
                    .map_err(|_err| {
                        nom::Err::Failure(nom::Context::Code(i, nom::ErrorKind::Custom(43)))
                    })
            })
            // Padding bytes
            >> take!(data_element_tag.padding_byte_size)
            >> (name)
    )
}

fn parse_dimensions_array_subelement(
    i: &[u8],
    endianness: nom::Endianness,
) -> IResult<&[u8], Dimensions> {
    do_parse!(
        i,
        data_element_tag: apply!(parse_data_element_tag, endianness)
            >> apply!(
                assert,
                data_element_tag.data_type == DataType::Int32
                    && data_element_tag.data_byte_size >= 8
                    && data_element_tag.data_byte_size % 4 == 0
            )
            >> dimensions:
                count!(
                    i32!(endianness),
                    (data_element_tag.data_byte_size / 4) as usize
                )
            // Padding bytes
            >> take!(data_element_tag.padding_byte_size)
            >> (dimensions)
    )
}

fn parse_array_flags_subelement(
    i: &[u8],
    endianness: nom::Endianness,
) -> IResult<&[u8], ArrayFlags> {
    do_parse!(
        i,
        tag_data_type: u32!(endianness)
            >> tag_data_len: u32!(endianness)
            >> apply!(
                assert,
                tag_data_type == DataType::UInt32 as u32 && tag_data_len == 8
            )
            >> flags_and_class: u32!(endianness)
            >> nzmax: u32!(endianness)
            >> (ArrayFlags {
                complex: (flags_and_class & 0x0800) != 0,
                global: (flags_and_class & 0x0400) != 0,
                logical: (flags_and_class & 0x0200) != 0,
                class: ArrayType::from_u8((flags_and_class & 0xFF) as u8).ok_or(
                    nom::Err::Failure(nom::Context::Code(i, nom::ErrorKind::Custom(44),))
                )?,
                nzmax: nzmax as usize,
            })
    )
}

fn parse_matrix_data_element(i: &[u8], endianness: nom::Endianness) -> IResult<&[u8], DataElement> {
    do_parse!(
        i,
        flags: apply!(parse_array_flags_subelement, endianness)
            >> data_element:
                switch!(value!(flags.class),
                     ArrayType::Cell => apply!(parse_unsupported_data_element, endianness)
                    | ArrayType::Struct => apply!(parse_structure_matrix_subelements, endianness, flags)
                    | ArrayType::Object => apply!(parse_unsupported_data_element, endianness)
                    | ArrayType::Char => apply!(parse_unsupported_data_element, endianness)
                    | ArrayType::Sparse => apply!(parse_sparse_matrix_subelements, endianness, flags)
                    | _ => apply!(parse_numeric_matrix_subelements, endianness, flags)
                )
            >> (data_element)
    )
}

fn numeric_data_types_are_compatible(array_type: DataType, subelement_type: DataType) -> bool {
    match array_type {
        DataType::Int8 => match subelement_type {
            DataType::Int8 => true,
            _ => false,
        },
        DataType::UInt8 => match subelement_type {
            DataType::UInt8 => true,
            _ => false,
        },
        DataType::Int16 => match subelement_type {
            DataType::UInt8 | DataType::Int16 => true,
            _ => false,
        },
        DataType::UInt16 => match subelement_type {
            DataType::UInt8 | DataType::UInt16 => true,
            _ => false,
        },
        DataType::Int32 => match subelement_type {
            DataType::UInt8 | DataType::Int16 | DataType::UInt16 | DataType::Int32 => true,
            _ => false,
        },
        DataType::UInt32 => match subelement_type {
            DataType::UInt8 | DataType::Int16 | DataType::UInt16 | DataType::UInt32 => true,
            _ => false,
        },
        DataType::Int64 => match subelement_type {
            DataType::UInt8
            | DataType::Int16
            | DataType::UInt16
            | DataType::Int32
            | DataType::Int64 => true,
            _ => false,
        },
        DataType::UInt64 => match subelement_type {
            DataType::UInt8
            | DataType::Int16
            | DataType::UInt16
            | DataType::Int32
            | DataType::UInt64 => true,
            _ => false,
        },
        DataType::Single => match subelement_type {
            DataType::UInt8
            | DataType::Int16
            | DataType::UInt16
            | DataType::Int32
            | DataType::Single => true,
            _ => false,
        },
        DataType::Double => match subelement_type {
            DataType::UInt8
            | DataType::Int16
            | DataType::UInt16
            | DataType::Int32
            | DataType::Double => true,
            _ => false,
        },
        _ => false,
    }
}

fn parse_numeric_subelement(i: &[u8], endianness: nom::Endianness) -> IResult<&[u8], NumericData> {
    do_parse!(
        i,
        data_element_tag: apply!(parse_data_element_tag, endianness)
            >> numeric_data:
                switch!(value!(data_element_tag.data_type),
                    DataType::Int8 => map!(switch!(value!(endianness),
                        nom::Endianness::Big => count!(be_i8, data_element_tag.data_byte_size as usize) |
                        nom::Endianness::Little => count!(le_i8, data_element_tag.data_byte_size as usize)
                    ), |data| NumericData::Int8(data)) |
                    DataType::UInt8 => map!(switch!(value!(endianness),
                        nom::Endianness::Big => count!(be_u8, data_element_tag.data_byte_size as usize) |
                        nom::Endianness::Little => count!(le_u8, data_element_tag.data_byte_size as usize)
                    ), |data| NumericData::UInt8(data)) |
                    DataType::Int16 => map!(switch!(value!(endianness),
                        nom::Endianness::Big => count!(be_i16, data_element_tag.data_byte_size as usize / 2) |
                        nom::Endianness::Little => count!(le_i16, data_element_tag.data_byte_size as usize / 2)
                    ), |data| NumericData::Int16(data)) |
                    DataType::UInt16 => map!(switch!(value!(endianness),
                        nom::Endianness::Big => count!(be_u16, data_element_tag.data_byte_size as usize / 2) |
                        nom::Endianness::Little => count!(le_u16, data_element_tag.data_byte_size as usize / 2)
                    ), |data| NumericData::UInt16(data)) |
                    DataType::Int32 => map!(switch!(value!(endianness),
                        nom::Endianness::Big => count!(be_i32, data_element_tag.data_byte_size as usize / 4) |
                        nom::Endianness::Little => count!(le_i32, data_element_tag.data_byte_size as usize / 4)
                    ), |data| NumericData::Int32(data)) |
                    DataType::UInt32 => map!(switch!(value!(endianness),
                        nom::Endianness::Big => count!(be_u32, data_element_tag.data_byte_size as usize / 4) |
                        nom::Endianness::Little => count!(le_u32, data_element_tag.data_byte_size as usize / 4)
                    ), |data| NumericData::UInt32(data)) |
                    DataType::Int64 => map!(switch!(value!(endianness),
                        nom::Endianness::Big => count!(be_i64, data_element_tag.data_byte_size as usize / 8) |
                        nom::Endianness::Little => count!(le_i64, data_element_tag.data_byte_size as usize / 8)
                    ), |data| NumericData::Int64(data)) |
                    DataType::UInt64 => map!(switch!(value!(endianness),
                        nom::Endianness::Big => count!(be_u64, data_element_tag.data_byte_size as usize / 8) |
                        nom::Endianness::Little => count!(le_u64, data_element_tag.data_byte_size as usize / 8)
                    ), |data| NumericData::UInt64(data)) |
                    DataType::Single => map!(switch!(value!(endianness),
                        nom::Endianness::Big => count!(be_f32, data_element_tag.data_byte_size as usize / 4) |
                        nom::Endianness::Little => count!(le_f32, data_element_tag.data_byte_size as usize / 4)
                    ), |data| NumericData::Single(data)) |
                    DataType::Double => map!(switch!(value!(endianness),
                        nom::Endianness::Big => count!(be_f64, data_element_tag.data_byte_size as usize / 8) |
                        nom::Endianness::Little => count!(le_f64, data_element_tag.data_byte_size as usize / 8)
                    ), |data| NumericData::Double(data))
                )
            // Padding bytes
            >> take!(data_element_tag.padding_byte_size)
            >> (numeric_data)
    )
}

fn parse_compressed_data_element(
    i: &[u8],
    endianness: nom::Endianness,
) -> IResult<&[u8], DataElement> {
    let mut buf = Vec::new();
    Decoder::new(i)
        .map_err(|err| {
            eprintln!("{:?}", err);
            nom::Err::Failure(nom::Context::Code(i, nom::ErrorKind::Custom(41)))
        })?
        .read_to_end(&mut buf)
        .map_err(|err| {
            eprintln!("{:?}", err);
            nom::Err::Failure(nom::Context::Code(i, nom::ErrorKind::Custom(42)))
        })?;
    let (_remaining, data_element) = parse_next_data_element(buf.as_slice(), endianness)
        .map_err(|err| replace_err_slice(err, i))?;
    Ok((&[], data_element))
}

pub type RowIndex = Vec<usize>;
pub type ColumnShift = Vec<usize>;

fn parse_numeric_matrix_subelements(
    i: &[u8],
    endianness: nom::Endianness,
    flags: ArrayFlags,
) -> IResult<&[u8], DataElement> {
    do_parse!(
        i,
        dimensions: apply!(parse_dimensions_array_subelement, endianness)
            >> name: apply!(parse_array_name_subelement, endianness)

            >> real_part: apply!(parse_numeric_subelement, endianness)
            // Check that size and type of the real part are correct
            >> n_required_elements: value!(dimensions.iter().product::<i32>())
            >> array_data_type: value!(flags.class.numeric_data_type().unwrap())
            >> apply!(assert, real_part.len() == n_required_elements as usize && numeric_data_types_are_compatible(array_data_type, real_part.data_type()))

            >> imag_part: cond!(flags.complex, apply!(parse_numeric_subelement, endianness))
            // Check that size and type of imaginary part are correct if present
            >> apply!(assert,
                if let Some(imag_part) = &imag_part {
                    imag_part.len() == n_required_elements as usize && numeric_data_types_are_compatible(array_data_type, imag_part.data_type())
                } else {
                    true
                }
            )

            >> (DataElement::NumericMatrix(
                flags, dimensions, name, real_part, imag_part
            ))
    )
}

fn parse_sparse_matrix_subelements(
    i: &[u8],
    endianness: nom::Endianness,
    flags: ArrayFlags,
) -> IResult<&[u8], DataElement> {
    // Figure out the type of array
    do_parse!(
        i,
        dimensions: apply!(parse_dimensions_array_subelement, endianness)
            >> name: apply!(parse_array_name_subelement, endianness)
            >> row_index: apply!(parse_row_index_array_subelement, endianness)
            >> column_index: apply!(parse_column_index_array_subelement, endianness)

            >> real_part: apply!(parse_numeric_subelement, endianness)
            // Check that size of the real part is correct (can't check for type in sparse matrices)
            >> apply!(assert, real_part.len() == flags.nzmax)

            >> imag_part: cond!(flags.complex, apply!(parse_numeric_subelement, endianness))
            // Check that size of the imaginary part is correct if present (can't check for type in sparse matrices)
            >> apply!(assert,
                if let Some(imag_part) = &imag_part {
                    imag_part.len() == flags.nzmax as usize
                } else {
                    true
                }
            )

            >> (DataElement::SparseMatrix(
                flags,
                dimensions,
                name,
                row_index.iter().map(|&i| i as usize).collect(),
                column_index.iter().map(|&i| i as usize).collect(),
                real_part,
                imag_part
            ))
    )
}

fn parse_row_index_array_subelement(
    i: &[u8],
    endianness: nom::Endianness,
) -> IResult<&[u8], RowIndex> {
    do_parse!(
        i,
        data_element_tag: apply!(parse_data_element_tag, endianness)
            >> apply!(
                assert,
                data_element_tag.data_type == DataType::Int32
                    && data_element_tag.data_byte_size > 0
            )
            >> row_index:
                count!(
                    i32!(endianness),
                    (data_element_tag.data_byte_size / 4) as usize
                )
            >> take!(data_element_tag.padding_byte_size)
            >> (row_index.iter().map(|&i| i as usize).collect())
    )
}

fn parse_column_index_array_subelement(
    i: &[u8],
    endianness: nom::Endianness,
) -> IResult<&[u8], ColumnShift> {
    do_parse!(
        i,
        data_element_tag: apply!(parse_data_element_tag, endianness)
            >> apply!(
                assert,
                data_element_tag.data_type == DataType::Int32
                    && data_element_tag.data_byte_size > 0
            )
            >> column_index:
                count!(
                    i32!(endianness),
                    (data_element_tag.data_byte_size / 4) as usize
                )
            >> take!(data_element_tag.padding_byte_size)
            >> (column_index.iter().map(|&i| i as usize).collect())
    )
}

//FIXME

pub type FieldNameLength = i32;
pub type FieldName = String;
pub type Fields = Vec<DataElement>;

fn parse_structure_matrix_subelements(
    i: &[u8],
    endianness: nom::Endianness,
    flags: ArrayFlags,
) -> IResult<&[u8], DataElement> {
    do_parse!(
        i,
        dimensions: apply!(parse_dimensions_array_subelement, endianness)
            >> name: apply!(parse_array_name_subelement, endianness)
            >> field_name_length: apply!(parse_field_name_length_subelement, endianness)
            >> fields_nb: value!(dimensions.iter().product::<i32>())
            >> field_names:
                count!(
                    apply!(parse_field_name, field_name_length as usize, endianness),
                    fields_nb as usize
                )
            >> fields:
                count!(
                    apply!(parse_matrix_data_element, endianness),
                    fields_nb as usize
                )
            >> (DataElement::StructureMatrix(
                flags,
                dimensions,
                name,
                field_name_length,
                field_names,
                fields
            ))
    )
}

fn parse_field_name_length_subelement(
    i: &[u8],
    endianness: nom::Endianness,
) -> IResult<&[u8], i32> {
    do_parse!(
        i,
        apply!(parse_data_element_tag, endianness) >> length: i32!(endianness) >> (length)
    )
}

fn parse_field_name(
    i: &[u8],
    length: usize,
    endianness: nom::Endianness,
) -> IResult<&[u8], FieldName> {
    do_parse!(
        i,
        name: map_res!(take_till!(|i| i == 0), |b| {
            std::str::from_utf8(b)
                .map(|s| s.to_owned())
                .map_err(|_err| {
                    nom::Err::Failure(nom::Context::Code(i, nom::ErrorKind::Custom(43)))
                })
        }) >> take!(length - name.len())
            >> (name)
    )
}

fn replace_context_slice<'old, 'new, E>(
    context: nom::Context<&'old [u8], E>,
    new_slice: &'new [u8],
) -> nom::Context<&'new [u8], E> {
    match context {
        nom::Context::Code(_, error_kind) => nom::Context::Code(new_slice, error_kind),
        nom::Context::List(list) => nom::Context::List(
            list.into_iter()
                .map(|(_, error_kind)| (new_slice, error_kind))
                .collect(),
        ),
    }
}

pub fn replace_err_slice<'old, 'new, E>(
    err: nom::Err<&'old [u8], E>,
    new_slice: &'new [u8],
) -> nom::Err<&'new [u8], E> {
    match err {
        nom::Err::Error(context) => nom::Err::Error(replace_context_slice(context, new_slice)),
        nom::Err::Failure(context) => nom::Err::Failure(replace_context_slice(context, new_slice)),
        nom::Err::Incomplete(needed) => nom::Err::Incomplete(needed),
    }
}

fn parse_unsupported_data_element(
    _i: &[u8],
    _endianness: nom::Endianness,
) -> IResult<&[u8], DataElement> {
    Ok((&[], DataElement::Unsupported))
}

#[derive(Debug)]
pub struct ParseResult {
    pub header: Header,
    pub data_elements: Vec<DataElement>,
}

pub fn parse_all(i: &[u8]) -> IResult<&[u8], ParseResult> {
    do_parse!(
        i,
        header: parse_header
            >> endianness: value!(if header.is_little_endian {
                nom::Endianness::Little
            } else {
                nom::Endianness::Big
            })
            >> data_elements: many0!(complete!(apply!(parse_next_data_element, endianness)))
            >> (ParseResult {
                header: header,
                data_elements: data_elements,
            })
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sparse1() {
        let data = include_bytes!("../tests/sparse1.mat");

        let (_, parsed_data) = parse_all(data).unwrap();
        let parsed_matrix_data = parsed_data.data_elements[0].clone();
        if let DataElement::SparseMatrix(flags, dim, name, irows, icols, real_vals, imag_vals) =
            parsed_matrix_data
        {
            assert_eq!(dim, vec![8, 8]);
            assert_eq!(irows, vec![5, 7, 2, 0, 1, 3, 6]);
            assert_eq!(icols, vec![0, 1, 2, 2, 3, 4, 5, 6, 7]);
            assert_eq!(
                real_vals,
                NumericData::Double(vec![2.0, 7.0, 4.0, 9.0, 5.0, 8.0, 6.0])
            );
            assert_eq!(imag_vals, None);
        } else {
            panic!("Error extracting DataElement::SparseMatrix");
        }
    }

    #[test]
    fn sparse2() {
        let data = include_bytes!("../tests/sparse2.mat");

        let (_, parsed_data) = parse_all(data).unwrap();
        let parsed_matrix_data = parsed_data.data_elements[0].clone();
        if let DataElement::SparseMatrix(flags, dim, name, irows, icols, real_vals, imag_vals) =
            parsed_matrix_data
        {
            assert_eq!(dim, vec![8, 8]);
            assert_eq!(irows, vec![5, 7, 2, 0, 1, 5, 3, 6]);
            assert_eq!(icols, vec![0, 1, 2, 2, 3, 4, 6, 7, 8]);
            assert_eq!(
                real_vals,
                NumericData::Double(vec![2.0, 7.0, 4.0, 9.0, 5.0, 6.0, 8.0, 6.0])
            );
            assert_eq!(
                imag_vals,
                Some(NumericData::Double(vec![
                    4.0, 0.0, 3.0, 7.0, 0.0, 1.0, 0.0, 0.0
                ]))
            );
        } else {
            panic!("Error extracting DataElement::SparseMatrix");
        }
    }

    #[test]
    fn structure1() {
        let data = include_bytes!("../tests/jgl009.mat");
        if let Ok((next1, header)) = parse_header(data) {
            // dbg!(header);
            let e = nom::Endianness::Little;
            if let Ok((next2, tag)) = parse_data_element_tag(next1, e) {
                // dbg!(tag);
                let mut buf = Vec::new();
                Decoder::new(next2).unwrap().read_to_end(&mut buf).unwrap();
                if let Ok((next3, tag2)) = parse_data_element_tag(&buf, e) {
                    // dbg!(tag2);
                    if let Ok((next4, flags)) = parse_array_flags_subelement(next3, e) {
                        // dbg!(flags);
                        if let Ok((next5, dimensions)) = parse_dimensions_array_subelement(next4, e)
                        {
                            // dbg!(dimensions);
                            if let Ok((next6, tag3)) = parse_data_element_tag(next4, e) {
                                dbg!(tag3);
                                let v = take!(next6, 8);
                                dbg!(v);
                                // let name = parse_array_name_subelement(next4, e);
                                // dbg!(name);
                            }
                        }
                    }
                }
            }
        }
    }
}
