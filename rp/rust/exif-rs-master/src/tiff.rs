
use std::fmt;
use mutate_once::MutOnce;

use crate::endian::{Endian, BigEndian, LittleEndian};
use crate::error::Error;
use crate::tag::{Context, Tag, UnitPiece};
use crate::util::{atou16, ctou32};
use crate::value;
use crate::value::Value;
use crate::value::get_type_info;

// TIFF 标头幻数 [EXIF23 4.5.2]。
const TIFF_BE: u16 = 0x4d4d;
const TIFF_LE: u16 = 0x4949;
const TIFF_FORTY_TWO: u16 = 0x002a;
pub const TIFF_BE_SIG: [u8; 4] = [0x4d, 0x4d, 0x00, 0x2a];
pub const TIFF_LE_SIG: [u8; 4] = [0x49, 0x49, 0x2a, 0x00];

// 部分解析的 TIFF 字段（IFD 条目）。
// Value::Unknown 被滥用来表示部分解析的值。
// 这样的值绝不能暴露给该库的用户。
#[derive(Debug)]
pub struct IfdEntry {
    // 部分解析时，该值存储为 Value::Unknown。
    // 不要将该字段泄漏到外部。
    field: MutOnce<Field>,
}

impl IfdEntry {
    pub fn ifd_num_tag(&self) -> (In, Tag) {
        if self.field.is_fixed() {
            let field = self.field.get_ref();
            (field.ifd_num, field.tag)
        } else {
            let field = self.field.get_mut();
            (field.ifd_num, field.tag)
        }
    }

    pub fn ref_field<'a>(&'a self, data: &[u8], le: bool) -> &'a Field {
        self.parse(data, le);
        self.field.get_ref()
    }

    fn into_field(self, data: &[u8], le: bool) -> Field {
        self.parse(data, le);
        self.field.into_inner()
    }

    fn parse(&self, data: &[u8], le: bool) {
        if !self.field.is_fixed() {
            let mut field = self.field.get_mut();
            if le {
                Self::parse_value::<LittleEndian>(&mut field.value, data);
            } else {
                Self::parse_value::<BigEndian>(&mut field.value, data);
            }
        }
    }

    // Converts a partially parsed value into a real one.
    fn parse_value<E>(value: &mut Value, data: &[u8]) where E: Endian {
        match *value {
            Value::Unknown(typ, cnt, ofs) => {
                let (unitlen, parser) = get_type_info::<E>(typ);
                if unitlen != 0 {
                    *value = parser(data, ofs as usize, cnt as usize);
                }
            },
            _ => panic!("value is already parsed"),
        }
    }
}

/// A TIFF/Exif field.
#[derive(Debug, Clone)]
pub struct Field {
    /// 该字段的标签。
    pub tag: Tag,
    /// 该字段所属的IFD的索引。
    pub ifd_num: In,
    /// 该字段的值。
    pub value: Value,
}

/// An IFD number.
///
/// The IFDs are indexed from 0.  The 0th IFD is for the primary image
/// and the 1st one is for the thumbnail.  Two associated constants,
/// `In::PRIMARY` and `In::THUMBNAIL`, are defined for them respectively.
///
/// # Examples
/// ```
/// use exif::In;
/// assert_eq!(In::PRIMARY.index(), 0);
/// assert_eq!(In::THUMBNAIL.index(), 1);
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct In(pub u16);

impl In {
    pub const PRIMARY: In = In(0);
    pub const THUMBNAIL: In = In(1);

    /// Returns the IFD number.
    #[inline]
    pub fn index(self) -> u16 {
        self.0
    }
}

impl fmt::Display for In {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            0 => f.pad("primary"),
            1 => f.pad("thumbnail"),
            n => f.pad(&format!("IFD{}", n)),
        }
    }
}

/// Parse the Exif attributes in the TIFF format.
///
/// Returns a Vec of Exif fields and a bool.
/// The boolean value is true if the data is little endian.
/// If an error occurred, `exif::Error` is returned.
pub fn parse_exif(data: &[u8]) -> Result<(Vec<Field>, bool), Error> {
    let mut parser = Parser::new();
    parser.parse(data)?;
    let (entries, le) = (parser.entries, parser.little_endian);
    Ok((entries.into_iter().map(|e| e.into_field(data, le)).collect(), le))
}

#[derive(Debug)]
pub struct Parser {
    pub entries: Vec<IfdEntry>,
    pub little_endian: bool,
    // `Some<Vec>` to enable the option and `None` to disable it.
    pub continue_on_error: Option<Vec<Error>>,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            little_endian: false,
            continue_on_error: None,
        }
    }

    pub fn parse(&mut self, data: &[u8]) -> Result<(), Error> {
        // 检查字节顺序并调用真正的解析器.
        if data.len() < 8 {
            return Err(Error::InvalidFormat("Truncated TIFF header"));
        }
        match BigEndian::loadu16(data, 0) {
            TIFF_BE => {
                self.little_endian = false;
                self.parse_header::<BigEndian>(data)
            },
            TIFF_LE => {
                self.little_endian = true;
                self.parse_header::<LittleEndian>(data)
            },
            _ => Err(Error::InvalidFormat("Invalid TIFF byte order")),
        }
    }

    fn parse_header<E>(&mut self, data: &[u8])
                       -> Result<(), Error> where E: Endian {
        // Parse the rest of the header (42 and the IFD offset).
        if E::loadu16(data, 2) != TIFF_FORTY_TWO {
            return Err(Error::InvalidFormat("Invalid forty two"));
        }
        let ifd_offset = E::loadu32(data, 4) as usize;
        self.parse_body::<E>(data, ifd_offset)
            .or_else(|e| self.check_error(e))
    }

    fn parse_body<E>(&mut self, data: &[u8], mut ifd_offset: usize)
                     -> Result<(), Error> where E: Endian {
        let mut ifd_num_ck = Some(0);
        while ifd_offset != 0 {
            let ifd_num = ifd_num_ck
                .ok_or(Error::InvalidFormat("Too many IFDs"))?;
            // Limit the number of IFDs to defend against resource exhaustion
            // attacks.
            if ifd_num >= 8 {
                return Err(Error::InvalidFormat("Limit the IFD count to 8"));
            }
            ifd_offset = self.parse_ifd::<E>(
                data, ifd_offset, Context::Tiff, ifd_num)?;
            ifd_num_ck = ifd_num.checked_add(1);
        }
        Ok(())
    }

    // Parse IFD [EXIF23 4.6.2].
    fn parse_ifd<E>(&mut self, data: &[u8],
                    mut offset: usize, ctx: Context, ifd_num: u16)
                    -> Result<usize, Error> where E: Endian {
        // Count (the number of the entries).
        if data.len() < offset || data.len() - offset < 2 {
            return Err(Error::InvalidFormat("Truncated IFD count"));
        }
        let count = E::loadu16(data, offset) as usize;
        offset += 2;

        // Array of entries.
        for _ in 0..count {
            if data.len() - offset < 12 {
                return Err(Error::InvalidFormat("Truncated IFD"));
            }
            let entry = Self::parse_ifd_entry::<E>(data, offset);
            offset += 12;
            let (tag, val) = match entry {
                Ok(x) => x,
                Err(e) => {
                    self.check_error(e)?;
                    continue;
                },
            };

            // No infinite recursion will occur because the context is not
            // recursively defined.
            let tag = Tag(ctx, tag);
            Tag::Artist.1
            let child_ctx = match tag {
                Tag::ExifIFDPointer => Context::Exif,
                Tag::GPSInfoIFDPointer => Context::Gps,
                Tag::InteropIFDPointer => Context::Interop,
                _ => {
                    self.entries.push(IfdEntry { field: Field {
                        tag: tag, ifd_num: In(ifd_num), value: val }.into()});
                    continue;
                },
            };
            self.parse_child_ifd::<E>(data, val, child_ctx, ifd_num)
                .or_else(|e| self.check_error(e))?;
        }

        // Offset to the next IFD.
        if data.len() - offset < 4 {
            return Err(Error::InvalidFormat("Truncated next IFD offset"));
        }
        let next_ifd_offset = E::loadu32(data, offset);
        Ok(next_ifd_offset as usize)
    }

    fn parse_ifd_entry<E>(data: &[u8], offset: usize)
                          -> Result<(u16, Value), Error> where E: Endian {
        // The size of entry has been checked in parse_ifd().
        let tag = E::loadu16(data, offset);
        let typ = E::loadu16(data, offset + 2);
        let cnt = E::loadu32(data, offset + 4);
        let valofs_at = offset + 8;
        let (unitlen, _parser) = get_type_info::<E>(typ);
        let vallen = unitlen.checked_mul(cnt as usize).ok_or(
            Error::InvalidFormat("Invalid entry count"))?;
        let val = if vallen <= 4 {
            Value::Unknown(typ, cnt, valofs_at as u32)
        } else {
            let ofs = E::loadu32(data, valofs_at) as usize;
            if data.len() < ofs || data.len() - ofs < vallen {
                return Err(Error::InvalidFormat("Truncated field value"));
            }
            Value::Unknown(typ, cnt, ofs as u32)
        };
        Ok((tag, val))
    }

    fn parse_child_ifd<E>(&mut self, data: &[u8],
                          mut pointer: Value, ctx: Context, ifd_num: u16)
                          -> Result<(), Error> where E: Endian {
        // 指针尚未解析，所以在这里解析。
        IfdEntry::parse_value::<E>(&mut pointer, data);

        // 指针字段的类型 == LONG 且计数 == 1，因此值（IFD 偏移量）必须嵌入该字段的“值偏移量”元素中。
        let ofs = pointer.get_uint(0).ok_or(
            Error::InvalidFormat("Invalid pointer"))? as usize;
        match self.parse_ifd::<E>(data, ofs, ctx, ifd_num)? {
            0 => Ok(()),
            _ => Err(Error::InvalidFormat("Unexpected next IFD")),
        }
    }

    fn check_error(&mut self, err: Error) -> Result<(), Error> {
        match self.continue_on_error {
            Some(ref mut v) => Ok(v.push(err)),
            None => Err(err),
        }
    }
}

pub fn is_tiff(buf: &[u8]) -> bool {
    buf.starts_with(&TIFF_BE_SIG) || buf.starts_with(&TIFF_LE_SIG)
}

/// A struct used to parse a DateTime field.
///
/// # Examples
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use exif::DateTime;
/// let dt = DateTime::from_ascii(b"2016:05:04 03:02:01")?;
/// assert_eq!(dt.year, 2016);
/// assert_eq!(dt.to_string(), "2016-05-04 03:02:01");
/// # Ok(()) }
/// ```
#[derive(Debug)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// The subsecond data in nanoseconds.  If the Exif attribute has
    /// more sigfinicant digits, they are rounded down.
    pub nanosecond: Option<u32>,
    /// The offset of the time zone in minutes.
    pub offset: Option<i16>,
}

impl DateTime {
    /// Parse an ASCII data of a DateTime field.  The range of a number
    /// is not validated, so, for example, 13 may be returned as the month.
    ///
    /// If the value is blank, `Error::BlankValue` is returned.
    pub fn from_ascii(data: &[u8]) -> Result<DateTime, Error> {
        if data == b"    :  :     :  :  " || data == b"                   " {
            return Err(Error::BlankValue("DateTime is blank"));
        } else if data.len() < 19 {
            return Err(Error::InvalidFormat("DateTime too short"));
        } else if !(data[4] == b':' && data[7] == b':' && data[10] == b' ' &&
                    data[13] == b':' && data[16] == b':') {
            return Err(Error::InvalidFormat("Invalid DateTime delimiter"));
        }
        Ok(DateTime {
            year: atou16(&data[0..4])?,
            month: atou16(&data[5..7])? as u8,
            day: atou16(&data[8..10])? as u8,
            hour: atou16(&data[11..13])? as u8,
            minute: atou16(&data[14..16])? as u8,
            second: atou16(&data[17..19])? as u8,
            nanosecond: None,
            offset: None,
        })
    }

    /// Parses an SubsecTime-like field.
    pub fn parse_subsec(&mut self, data: &[u8]) -> Result<(), Error> {
        let mut subsec = 0;
        let mut ndigits = 0;
        for &c in data {
            if c == b' ' {
                break;
            }
            subsec = subsec * 10 + ctou32(c)?;
            ndigits += 1;
            if ndigits >= 9 {
                break;
            }
        }
        if ndigits == 0 {
            self.nanosecond = None;
        } else {
            for _ in ndigits..9 {
                subsec *= 10;
            }
            self.nanosecond = Some(subsec);
        }
        Ok(())
    }

    /// Parses an OffsetTime-like field.
    pub fn parse_offset(&mut self, data: &[u8]) -> Result<(), Error> {
        if data == b"   :  " || data == b"      " {
            return Err(Error::BlankValue("OffsetTime is blank"));
        } else if data.len() < 6 {
            return Err(Error::InvalidFormat("OffsetTime too short"));
        } else if data[3] != b':' {
            return Err(Error::InvalidFormat("Invalid OffsetTime delimiter"));
        }
        let hour = atou16(&data[1..3])?;
        let min = atou16(&data[4..6])?;
        let offset = (hour * 60 + min) as i16;
        self.offset = Some(match data[0] {
            b'+' => offset,
            b'-' => -offset,
            _ => return Err(Error::InvalidFormat("Invalid OffsetTime sign")),
        });
        Ok(())
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
               self.year, self.month, self.day,
               self.hour, self.minute, self.second)
    }
}

impl Field {
    /// Returns an object that implements `std::fmt::Display` for
    /// printing the value of this field in a tag-specific format.
    ///
    /// To print the value with the unit, call `with_unit` method on the
    /// returned object.  It takes a parameter, which is either `()`,
    /// `&Field`, or `&Exif`, that provides the unit information.
    /// If the unit does not depend on another field, `()` can be used.
    /// Otherwise, `&Field` or `&Exif` should be used.
    ///
    /// # Examples
    ///
    /// ```
    /// use exif::{Field, In, Tag, Value};
    ///
    /// let xres = Field {
    ///     tag: Tag::XResolution,
    ///     ifd_num: In::PRIMARY,
    ///     value: Value::Rational(vec![(72, 1).into()]),
    /// };
    /// let resunit = Field {
    ///     tag: Tag::ResolutionUnit,
    ///     ifd_num: In::PRIMARY,
    ///     value: Value::Short(vec![3]),
    /// };
    /// assert_eq!(xres.display_value().to_string(), "72");
    /// assert_eq!(resunit.display_value().to_string(), "cm");
    /// // The unit of XResolution is indicated by ResolutionUnit.
    /// assert_eq!(xres.display_value().with_unit(&resunit).to_string(),
    ///            "72 pixels per cm");
    /// // If ResolutionUnit is not given, the default value is used.
    /// assert_eq!(xres.display_value().with_unit(()).to_string(),
    ///            "72 pixels per inch");
    /// assert_eq!(xres.display_value().with_unit(&xres).to_string(),
    ///            "72 pixels per inch");
    ///
    /// let flen = Field {
    ///     tag: Tag::FocalLengthIn35mmFilm,
    ///     ifd_num: In::PRIMARY,
    ///     value: Value::Short(vec![24]),
    /// };
    /// // The unit of the focal length is always mm, so the argument
    /// // has nothing to do with the result.
    /// assert_eq!(flen.display_value().with_unit(()).to_string(),
    ///            "24 mm");
    /// assert_eq!(flen.display_value().with_unit(&resunit).to_string(),
    ///            "24 mm");
    /// ```
    #[inline]
    pub fn display_value(&self) -> DisplayValue {
        DisplayValue {
            tag: self.tag,
            ifd_num: self.ifd_num,
            value_display: self.value.display_as(self.tag),
        }
    }
}

/// Helper struct for printing a value in a tag-specific format.
pub struct DisplayValue<'a> {
    tag: Tag,
    ifd_num: In,
    value_display: value::Display<'a>,
}

impl<'a> DisplayValue<'a> {
    #[inline]
    pub fn with_unit<T>(&self, unit_provider: T)
                        -> DisplayValueUnit<'a, T> where T: ProvideUnit<'a> {
        DisplayValueUnit {
            ifd_num: self.ifd_num,
            value_display: self.value_display,
            unit: self.tag.unit(),
            unit_provider: unit_provider,
        }
    }
}

impl<'a> fmt::Display for DisplayValue<'a> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value_display.fmt(f)
    }
}

/// Helper struct for printing a value with its unit.
pub struct DisplayValueUnit<'a, T> where T: ProvideUnit<'a> {
    ifd_num: In,
    value_display: value::Display<'a>,
    unit: Option<&'static [UnitPiece]>,
    unit_provider: T,
}

impl<'a, T> fmt::Display for DisplayValueUnit<'a, T> where T: ProvideUnit<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(unit) = self.unit {
            assert!(!unit.is_empty());
            for piece in unit {
                match *piece {
                    UnitPiece::Value => self.value_display.fmt(f),
                    UnitPiece::Str(s) => f.write_str(s),
                    UnitPiece::Tag(tag) =>
                        if let Some(x) = self.unit_provider.get_field(
                                tag, self.ifd_num) {
                            x.value.display_as(tag).fmt(f)
                        } else if let Some(x) = tag.default_value() {
                            x.display_as(tag).fmt(f)
                        } else {
                            write!(f, "[{} missing]", tag)
                        },
                }?
            }
            Ok(())
        } else {
            self.value_display.fmt(f)
        }
    }
}

pub trait ProvideUnit<'a>: Copy {
    fn get_field(self, tag: Tag, ifd_num: In) -> Option<&'a Field>;
}

impl<'a> ProvideUnit<'a> for () {
    fn get_field(self, _tag: Tag, _ifd_num: In) -> Option<&'a Field> {
        None
    }
}

impl<'a> ProvideUnit<'a> for &'a Field {
    fn get_field(self, tag: Tag, ifd_num: In) -> Option<&'a Field> {
        Some(self).filter(|x| x.tag == tag && x.ifd_num == ifd_num)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_convert() {
        assert_eq!(In::PRIMARY.index(), 0);
        assert_eq!(In::THUMBNAIL.index(), 1);
        assert_eq!(In(2).index(), 2);
        assert_eq!(In(65535).index(), 65535);
        assert_eq!(In::PRIMARY, In(0));
    }

    #[test]
    fn in_display() {
        assert_eq!(format!("{:10}", In::PRIMARY), "primary   ");
        assert_eq!(format!("{:>10}", In::THUMBNAIL), " thumbnail");
        assert_eq!(format!("{:10}", In(2)), "IFD2      ");
        assert_eq!(format!("{:^10}", In(65535)), " IFD65535 ");
    }

    #[test]
    fn truncated() {
        let mut data =
            b"MM\0\x2a\0\0\0\x08\
              \0\x01\x01\0\0\x03\0\0\0\x01\0\x14\0\0\0\0\0\0".to_vec();
        parse_exif(&data).unwrap();
        while let Some(_) = data.pop() {
            parse_exif(&data).unwrap_err();
        }
    }

    // Before the error is returned, the IFD is parsed multiple times
    // as the 0th, 1st, ..., and n-th IFDs.
    #[test]
    fn inf_loop_by_next() {
        let data = b"MM\0\x2a\0\0\0\x08\
                     \0\x01\x01\0\0\x03\0\0\0\x01\0\x14\0\0\0\0\0\x08";
        assert_err_pat!(parse_exif(data),
                        Error::InvalidFormat("Limit the IFD count to 8"));
    }

    #[test]
    fn inf_loop_by_exif_next() {
        let data = b"MM\x00\x2a\x00\x00\x00\x08\
                     \x00\x01\x87\x69\x00\x04\x00\x00\x00\x01\x00\x00\x00\x1a\
                     \x00\x00\x00\x00\
                     \x00\x01\x90\x00\x00\x07\x00\x00\x00\x040231\
                     \x00\x00\x00\x08";
        assert_err_pat!(parse_exif(data),
                        Error::InvalidFormat("Unexpected next IFD"));
    }

    #[test]
    fn unknown_field() {
        let data = b"MM\0\x2a\0\0\0\x08\
                     \0\x01\x01\0\xff\xff\0\0\0\x01\0\x14\0\0\0\0\0\0";
        let (v, _le) = parse_exif(data).unwrap();
        assert_eq!(v.len(), 1);
        assert_pat!(v[0].value, Value::Unknown(0xffff, 1, 0x12));
    }

    #[test]
    fn parse_ifd_entry() {
        // BYTE (type == 1)
        let data = b"\x02\x03\x00\x01\0\0\0\x04ABCD";
        assert_pat!(Parser::parse_ifd_entry::<BigEndian>(data, 0).unwrap(),
                    (0x0203, Value::Unknown(1, 4, 8)));
        let data = b"\x02\x03\x00\x01\0\0\0\x05\0\0\0\x0cABCDE";
        assert_pat!(Parser::parse_ifd_entry::<BigEndian>(data, 0).unwrap(),
                    (0x0203, Value::Unknown(1, 5, 12)));
        let data = b"\x02\x03\x00\x01\0\0\0\x05\0\0\0\x0cABCD";
        assert_err_pat!(Parser::parse_ifd_entry::<BigEndian>(data, 0),
                        Error::InvalidFormat("Truncated field value"));

        // SHORT (type == 3)
        let data = b"X\x04\x05\x00\x03\0\0\0\x02ABCD";
        assert_pat!(Parser::parse_ifd_entry::<BigEndian>(data, 1).unwrap(),
                    (0x0405, Value::Unknown(3, 2, 9)));
        let data = b"X\x04\x05\x00\x03\0\0\0\x03\0\0\0\x0eXABCDEF";
        assert_pat!(Parser::parse_ifd_entry::<BigEndian>(data, 1).unwrap(),
                    (0x0405, Value::Unknown(3, 3, 14)));
        let data = b"X\x04\x05\x00\x03\0\0\0\x03\0\0\0\x0eXABCDE";
        assert_err_pat!(Parser::parse_ifd_entry::<BigEndian>(data, 1),
                        Error::InvalidFormat("Truncated field value"));

        // Really unknown
        let data = b"X\x01\x02\x03\x04\x05\x06\x07\x08ABCD";
        assert_pat!(Parser::parse_ifd_entry::<BigEndian>(data, 1).unwrap(),
                    (0x0102, Value::Unknown(0x0304, 0x05060708, 9)));
    }

    #[test]
    fn date_time() {
        let mut dt = DateTime::from_ascii(b"2016:05:04 03:02:01").unwrap();
        assert_eq!(dt.year, 2016);
        assert_eq!(dt.to_string(), "2016-05-04 03:02:01");

        dt.parse_subsec(b"987").unwrap();
        assert_eq!(dt.nanosecond.unwrap(), 987000000);
        dt.parse_subsec(b"000987").unwrap();
        assert_eq!(dt.nanosecond.unwrap(), 987000);
        dt.parse_subsec(b"987654321").unwrap();
        assert_eq!(dt.nanosecond.unwrap(), 987654321);
        dt.parse_subsec(b"9876543219").unwrap();
        assert_eq!(dt.nanosecond.unwrap(), 987654321);
        dt.parse_subsec(b"130   ").unwrap();
        assert_eq!(dt.nanosecond.unwrap(), 130000000);
        dt.parse_subsec(b"0").unwrap();
        assert_eq!(dt.nanosecond.unwrap(), 0);
        dt.parse_subsec(b"").unwrap();
        assert!(dt.nanosecond.is_none());
        dt.parse_subsec(b" ").unwrap();
        assert!(dt.nanosecond.is_none());

        dt.parse_offset(b"+00:00").unwrap();
        assert_eq!(dt.offset.unwrap(), 0);
        dt.parse_offset(b"+01:23").unwrap();
        assert_eq!(dt.offset.unwrap(), 83);
        dt.parse_offset(b"+99:99").unwrap();
        assert_eq!(dt.offset.unwrap(), 6039);
        dt.parse_offset(b"-01:23").unwrap();
        assert_eq!(dt.offset.unwrap(), -83);
        dt.parse_offset(b"-99:99").unwrap();
        assert_eq!(dt.offset.unwrap(), -6039);
        assert_err_pat!(dt.parse_offset(b"   :  "), Error::BlankValue(_));
        assert_err_pat!(dt.parse_offset(b"      "), Error::BlankValue(_));
    }

    #[test]
    fn display_value_with_unit() {
        let cm = Field {
            tag: Tag::ResolutionUnit,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![3]),
        };
        let cm_tn = Field {
            tag: Tag::ResolutionUnit,
            ifd_num: In::THUMBNAIL,
            value: Value::Short(vec![3]),
        };
        // No unit.
        let exifver = Field {
            tag: Tag::ExifVersion,
            ifd_num: In::PRIMARY,
            value: Value::Undefined(b"0231".to_vec(), 0),
        };
        assert_eq!(exifver.display_value().to_string(),
                   "2.31");
        assert_eq!(exifver.display_value().with_unit(()).to_string(),
                   "2.31");
        assert_eq!(exifver.display_value().with_unit(&cm).to_string(),
                   "2.31");
        // Fixed string.
        let width = Field {
            tag: Tag::ImageWidth,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![257]),
        };
        assert_eq!(width.display_value().to_string(),
                   "257");
        assert_eq!(width.display_value().with_unit(()).to_string(),
                   "257 pixels");
        assert_eq!(width.display_value().with_unit(&cm).to_string(),
                   "257 pixels");
        // Unit tag (with a non-default value).
        // Unit tag is missing but the default is specified.
        let xres = Field {
            tag: Tag::XResolution,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![(300, 1).into()]),
        };
        assert_eq!(xres.display_value().to_string(),
                   "300");
        assert_eq!(xres.display_value().with_unit(()).to_string(),
                   "300 pixels per inch");
        assert_eq!(xres.display_value().with_unit(&cm).to_string(),
                   "300 pixels per cm");
        assert_eq!(xres.display_value().with_unit(&cm_tn).to_string(),
                   "300 pixels per inch");
        // Unit tag is missing and the default is not specified.
        let gpslat = Field {
            tag: Tag::GPSLatitude,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![
                (10, 1).into(), (0, 1).into(), (1, 10).into()]),
        };
        assert_eq!(gpslat.display_value().to_string(),
                   "10 deg 0 min 0.1 sec");
        assert_eq!(gpslat.display_value().with_unit(()).to_string(),
                   "10 deg 0 min 0.1 sec [GPSLatitudeRef missing]");
        assert_eq!(gpslat.display_value().with_unit(&cm).to_string(),
                   "10 deg 0 min 0.1 sec [GPSLatitudeRef missing]");
    }

    #[test]
    fn no_borrow_no_move() {
        let resunit = Field {
            tag: Tag::ResolutionUnit,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![3]),
        };
        // This fails to compile with "temporary value dropped while
        // borrowed" error if with_unit() borrows self.
        let d = resunit.display_value().with_unit(());
        assert_eq!(d.to_string(), "cm");
        // This fails to compile if with_unit() moves self.
        let d1 = resunit.display_value();
        let d2 = d1.with_unit(());
        assert_eq!(d1.to_string(), "cm");
        assert_eq!(d2.to_string(), "cm");
    }

    #[test]
    fn continue_on_error() {
        macro_rules! define_test {
            {
                data: $data:expr,
                fields: [$($fields:pat),*],
                errors: [$first_error:pat $(, $rest_errors:pat)*]
            } => {
                let data = $data;
                let mut parser = Parser::new();
                assert_err_pat!(parser.parse(data), $first_error);
                let mut parser = Parser::new();
                parser.continue_on_error = Some(Vec::new());
                parser.parse(data).unwrap();
                assert_eq!(parser.little_endian, false);
                let mut entries = parser.entries.iter();
                $(
                    assert_pat!(entries.next().unwrap()
                                       .ref_field(data, parser.little_endian),
                                $fields);
                )*
                assert_pat!(entries.next(), None);
                let mut errors =
                    parser.continue_on_error.as_ref().unwrap().iter();
                assert_pat!(errors.next().unwrap(), $first_error);
                $(
                    assert_pat!(errors.next().unwrap(), $rest_errors);
                )*
                assert_pat!(errors.next(), None);
            }
        }
        // 0th IFD is missing.
        define_test! {
            data: b"MM\0\x2a\0\0\0\x08",
            fields: [],
            errors: [Error::InvalidFormat("Truncated IFD count")]
        }
        // 2nd entry is truncated.
        define_test! {
            data: b"MM\0\x2a\0\0\0\x08\
                    \0\x02\x01\x00\0\x03\0\0\0\x01\0\x14\0\0\
                          \x01\x01\0\x03\0\0\0\x01\0\x15\0",
            fields: [Field { tag: Tag::ImageWidth, ifd_num: In(0),
                             value: Value::Short(_) }],
            errors: [Error::InvalidFormat("Truncated IFD")]
        }
        // 1st entry broken.
        define_test! {
            data: b"MM\0\x2a\0\0\0\x08\
                    \0\x02\x01\x00\0\x03\0\0\0\x03\0\0\0\x21\
                          \x01\x01\0\x03\0\0\0\x01\0\x15\0\0\
                          \0\0\0\0",
            fields: [Field { tag: Tag::ImageLength, ifd_num: In(0),
                             value: Value::Short(_) }],
            errors: [Error::InvalidFormat("Truncated field value")]
        }
        // Exif IFD has non-zero next IFD offset.
        // Top-level next IFD is also broken.
        define_test! {
            data: b"MM\0\x2a\0\0\0\x08\
                    \0\x02\x87\x69\0\x04\0\0\0\x01\0\0\0\x26\
                          \xfd\xe8\0\x09\0\0\0\x01\xfe\xdc\xba\x98\
                          \xff\xff\xff\xff\
                    \0\x01\x90\x00\0\x07\0\0\0\x04\x00\x02\x03\x02\
                          \0\0\0\x01",
            fields: [Field { tag: Tag::ExifVersion, ifd_num: In(0),
                             value: Value::Undefined(_, _) },
                     Field { tag: Tag(Context::Tiff, 65000), ifd_num: In(0),
                             value: Value::SLong(_) }],
            errors: [Error::InvalidFormat("Unexpected next IFD"),
                     Error::InvalidFormat("Truncated IFD count")]
        }
        // Exif IFD pointer has a bad type.
        define_test! {
            data: b"MM\0\x2a\0\0\0\x08\
                    \0\x02\x87\x69\0\x09\0\0\0\x01\0\0\0\x26\
                          \xfd\xe8\0\x06\0\0\0\x03\xfe\xdc\xba\x98\
                          \0\0\0\0\
                    \0\x01\x90\x00\0\x07\0\0\0\x04\x00\x02\x03\x02\
                          \0\0\0\x01",
            fields: [Field { tag: Tag(Context::Tiff, 65000), ifd_num: In(0),
                             value: Value::SByte(_) }],
            errors: [Error::InvalidFormat("Invalid pointer")]
        }
        // Exif IFD pointer is empty.
        define_test! {
            data: b"MM\0\x2a\0\0\0\x08\
                    \0\x02\x87\x69\0\x04\0\0\0\x00\0\0\0\x26\
                          \xfd\xe8\0\x08\0\0\0\x02\xfe\xdc\xba\x98\
                          \0\0\0\0\
                    \0\x01\x90\x00\0\x07\0\0\0\x04\x00\x02\x03\x02\
                          \0\0\0\x01",
            fields: [Field { tag: Tag(Context::Tiff, 65000), ifd_num: In(0),
                             value: Value::SShort(_) }],
            errors: [Error::InvalidFormat("Invalid pointer")]
        }
    }
}
