//! Read the package name and version from an Android App Bundle.
//!
//! An AAB is a zip archive whose `base/manifest/AndroidManifest.xml` is an
//! aapt2 protobuf `XmlNode`, so this module carries a minimal zip reader and
//! protobuf decoder instead of depending on bundletool.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, bail};
use flate2::read::DeflateDecoder;

const MANIFEST_ENTRY: &str = "base/manifest/AndroidManifest.xml";
const ANDROID_NAMESPACE: &str = "http://schemas.android.com/apk/res/android";
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AabManifest {
    pub package_name: String,
    pub version_code: i64,
    pub version_name: Option<String>,
}

/// Read `package`, `android:versionCode` and `android:versionName` from an AAB.
pub fn read_aab_manifest(path: &Path) -> anyhow::Result<AabManifest> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let manifest = read_zip_entry(&mut file, MANIFEST_ENTRY)
        .with_context(|| format!("failed to read {MANIFEST_ENTRY} from {}", path.display()))?;
    parse_manifest(&manifest)
        .with_context(|| format!("failed to parse the manifest in {}", path.display()))
}

// ── Zip ─────────────────────────────────────────────────────────

fn u16_at(bytes: &[u8], offset: usize) -> anyhow::Result<u16> {
    bytes
        .get(offset..offset + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .context("truncated zip record")
}

fn u32_at(bytes: &[u8], offset: usize) -> anyhow::Result<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .context("truncated zip record")
}

fn read_exact_at(file: &mut File, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buffer = vec![0; len];
    file.read_exact(&mut buffer)?;
    Ok(buffer)
}

fn read_zip_entry(file: &mut File, wanted: &str) -> anyhow::Result<Vec<u8>> {
    let file_len = file.metadata()?.len();
    // End of central directory: 22 bytes plus a comment of at most 64 KiB.
    let tail_len = file_len.min(22 + 65_535);
    let tail = read_exact_at(file, file_len - tail_len, tail_len as usize)?;
    let eocd = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| tail[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .context("not a zip archive")?;
    let cd_size = u32_at(&tail, eocd + 12)?;
    let cd_offset = u32_at(&tail, eocd + 16)?;
    if cd_size == u32::MAX || cd_offset == u32::MAX {
        bail!("zip64 bundles are not supported");
    }
    let directory = read_exact_at(file, u64::from(cd_offset), cd_size as usize)?;

    let mut cursor = 0;
    while cursor + 46 <= directory.len() {
        if u32_at(&directory, cursor)? != 0x0201_4b50 {
            bail!("corrupt zip central directory");
        }
        let method = u16_at(&directory, cursor + 10)?;
        let compressed_size = u32_at(&directory, cursor + 20)?;
        let uncompressed_size = u32_at(&directory, cursor + 24)?;
        let name_len = usize::from(u16_at(&directory, cursor + 28)?);
        let extra_len = usize::from(u16_at(&directory, cursor + 30)?);
        let comment_len = usize::from(u16_at(&directory, cursor + 32)?);
        let local_offset = u32_at(&directory, cursor + 42)?;
        let name = directory
            .get(cursor + 46..cursor + 46 + name_len)
            .context("truncated zip entry name")?;
        cursor += 46 + name_len + extra_len + comment_len;
        if name != wanted.as_bytes() {
            continue;
        }

        if u64::from(uncompressed_size) > MAX_MANIFEST_BYTES {
            bail!("{wanted} is unexpectedly large");
        }
        let header = read_exact_at(file, u64::from(local_offset), 30)?;
        if u32_at(&header, 0)? != 0x0403_4b50 {
            bail!("corrupt zip local header");
        }
        let data_offset = u64::from(local_offset)
            + 30
            + u64::from(u16_at(&header, 26)?)
            + u64::from(u16_at(&header, 28)?);
        let compressed = read_exact_at(file, data_offset, compressed_size as usize)?;
        return match method {
            0 => Ok(compressed),
            8 => {
                let mut out = Vec::with_capacity(uncompressed_size as usize);
                DeflateDecoder::new(compressed.as_slice())
                    .take(MAX_MANIFEST_BYTES)
                    .read_to_end(&mut out)?;
                Ok(out)
            }
            other => bail!("unsupported zip compression method {other}"),
        };
    }
    bail!("{wanted} not found; is this an Android App Bundle?")
}

// ── Protobuf ────────────────────────────────────────────────────

enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

struct Fields<'a> {
    bytes: &'a [u8],
}

fn read_varint(bytes: &[u8], cursor: &mut usize) -> anyhow::Result<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*cursor).context("truncated varint")?;
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    bail!("varint is too long")
}

impl<'a> Fields<'a> {
    fn next_field(&mut self) -> anyhow::Result<Option<(u64, Value<'a>)>> {
        if self.bytes.is_empty() {
            return Ok(None);
        }
        let mut cursor = 0;
        let key = read_varint(self.bytes, &mut cursor)?;
        let value = match key & 7 {
            0 => Value::Varint(read_varint(self.bytes, &mut cursor)?),
            1 => {
                cursor += 8;
                Value::Fixed
            }
            2 => {
                let len = usize::try_from(read_varint(self.bytes, &mut cursor)?)?;
                let value = self
                    .bytes
                    .get(cursor..cursor + len)
                    .context("truncated protobuf field")?;
                cursor += len;
                Value::Bytes(value)
            }
            5 => {
                cursor += 4;
                Value::Fixed
            }
            other => bail!("unsupported protobuf wire type {other}"),
        };
        if cursor > self.bytes.len() {
            bail!("truncated protobuf field");
        }
        self.bytes = &self.bytes[cursor..];
        Ok(Some((key >> 3, value)))
    }
}

fn fields(bytes: &[u8]) -> Fields<'_> {
    Fields { bytes }
}

#[derive(Default)]
struct Attribute {
    namespace_uri: String,
    name: String,
    value: String,
    compiled_int: Option<i64>,
    compiled_string: Option<String>,
}

/// Decode an aapt2 `Item`: `str` (2) holds a `String { value = 1 }` and
/// `prim` (7) holds a `Primitive` whose int fields are 6 (decimal) and 7 (hex).
fn decode_compiled_item(bytes: &[u8], attribute: &mut Attribute) -> anyhow::Result<()> {
    let mut item = fields(bytes);
    while let Some((number, value)) = item.next_field()? {
        match (number, value) {
            (2, Value::Bytes(string)) => {
                let mut string = fields(string);
                while let Some((1, Value::Bytes(text))) = string.next_field()? {
                    attribute.compiled_string = Some(String::from_utf8_lossy(text).into_owned());
                }
            }
            (7, Value::Bytes(primitive)) => {
                let mut primitive = fields(primitive);
                while let Some((number, value)) = primitive.next_field()? {
                    if let (6 | 7, Value::Varint(raw)) = (number, value) {
                        attribute.compiled_int = Some(i64::from(raw as u32 as i32));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn decode_attribute(bytes: &[u8]) -> anyhow::Result<Attribute> {
    let mut attribute = Attribute::default();
    let mut message = fields(bytes);
    while let Some((number, value)) = message.next_field()? {
        match (number, value) {
            (1, Value::Bytes(text)) => {
                attribute.namespace_uri = String::from_utf8_lossy(text).into_owned();
            }
            (2, Value::Bytes(text)) => attribute.name = String::from_utf8_lossy(text).into_owned(),
            (3, Value::Bytes(text)) => {
                attribute.value = String::from_utf8_lossy(text).into_owned();
            }
            (6, Value::Bytes(item)) => decode_compiled_item(item, &mut attribute)?,
            _ => {}
        }
    }
    Ok(attribute)
}

fn parse_manifest(bytes: &[u8]) -> anyhow::Result<AabManifest> {
    // XmlNode.element (1) → XmlElement { name = 3, attribute = 4 }.
    let mut node = fields(bytes);
    let mut element = None;
    while let Some((number, value)) = node.next_field()? {
        if let (1, Value::Bytes(bytes)) = (number, value) {
            element = Some(bytes);
        }
    }
    let element = element.context("manifest has no root element")?;

    let mut name = String::new();
    let mut attributes = Vec::new();
    let mut message = fields(element);
    while let Some((number, value)) = message.next_field()? {
        match (number, value) {
            (3, Value::Bytes(text)) => name = String::from_utf8_lossy(text).into_owned(),
            (4, Value::Bytes(bytes)) => attributes.push(decode_attribute(bytes)?),
            _ => {}
        }
    }
    if name != "manifest" {
        bail!("root element is <{name}>, expected <manifest>");
    }

    let find = |namespace: &str, wanted: &str| {
        attributes
            .iter()
            .find(|attribute| attribute.namespace_uri == namespace && attribute.name == wanted)
    };
    let package_name = find("", "package")
        .map(|attribute| attribute.value.clone())
        .filter(|value| !value.is_empty())
        .context("manifest has no package attribute")?;
    let version_code_attribute =
        find(ANDROID_NAMESPACE, "versionCode").context("manifest has no android:versionCode")?;
    let version_code = match version_code_attribute.compiled_int {
        Some(code) => code,
        None => version_code_attribute
            .value
            .trim()
            .parse()
            .context("android:versionCode is not a number")?,
    };
    let version_name = find(ANDROID_NAMESPACE, "versionName").and_then(|attribute| {
        Some(attribute.value.clone())
            .filter(|value| !value.is_empty())
            .or_else(|| attribute.compiled_string.clone())
    });

    Ok(AabManifest {
        package_name,
        version_code,
        version_name,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::DeflateEncoder;

    use super::*;

    fn varint(mut value: u64, out: &mut Vec<u8>) {
        while value >= 0x80 {
            out.push((value as u8) | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    fn bytes_field(number: u64, payload: &[u8], out: &mut Vec<u8>) {
        varint(number << 3 | 2, out);
        varint(payload.len() as u64, out);
        out.extend_from_slice(payload);
    }

    fn attribute(namespace: &str, name: &str, value: &str, item: Option<Vec<u8>>) -> Vec<u8> {
        let mut out = Vec::new();
        if !namespace.is_empty() {
            bytes_field(1, namespace.as_bytes(), &mut out);
        }
        bytes_field(2, name.as_bytes(), &mut out);
        if !value.is_empty() {
            bytes_field(3, value.as_bytes(), &mut out);
        }
        if let Some(item) = item {
            bytes_field(6, &item, &mut out);
        }
        out
    }

    fn manifest_proto() -> Vec<u8> {
        // versionCode only as a compiled decimal primitive, versionName as a raw value.
        let mut primitive = Vec::new();
        varint(6 << 3, &mut primitive);
        varint(4_210_007, &mut primitive);
        let mut item = Vec::new();
        bytes_field(7, &primitive, &mut item);

        let mut element = Vec::new();
        bytes_field(3, b"manifest", &mut element);
        bytes_field(
            4,
            &attribute("", "package", "build.oore.demo", None),
            &mut element,
        );
        bytes_field(
            4,
            &attribute(ANDROID_NAMESPACE, "versionCode", "", Some(item)),
            &mut element,
        );
        bytes_field(
            4,
            &attribute(ANDROID_NAMESPACE, "versionName", "4.2.0", None),
            &mut element,
        );
        let mut node = Vec::new();
        bytes_field(1, &element, &mut node);
        node
    }

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut directory = Vec::new();
        for (name, data) in entries {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(data).unwrap();
            let compressed = encoder.finish().unwrap();
            let offset = out.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&compressed);

            directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            directory.extend_from_slice(&[20, 0, 20, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            directory.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            directory.extend_from_slice(&(data.len() as u32).to_le_bytes());
            directory.extend_from_slice(&(name.len() as u16).to_le_bytes());
            directory.extend_from_slice(&[0; 12]);
            directory.extend_from_slice(&offset.to_le_bytes());
            directory.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&directory);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn reads_package_and_version_from_bundle() {
        let dir = std::env::temp_dir().join(format!("oore-aab-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app-release.aab");
        let manifest = manifest_proto();
        std::fs::write(
            &path,
            zip_with(&[
                ("BundleConfig.pb", b"ignored"),
                (MANIFEST_ENTRY, manifest.as_slice()),
            ]),
        )
        .unwrap();

        let parsed = read_aab_manifest(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(
            parsed,
            AabManifest {
                package_name: "build.oore.demo".to_string(),
                version_code: 4_210_007,
                version_name: Some("4.2.0".to_string()),
            }
        );
    }

    #[test]
    fn rejects_archive_without_manifest() {
        let dir = std::env::temp_dir().join(format!("oore-aab-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not-a-bundle.aab");
        std::fs::write(&path, zip_with(&[("classes.dex", b"dex")])).unwrap();
        let error = format!("{:#}", read_aab_manifest(&path).unwrap_err());
        std::fs::remove_dir_all(&dir).ok();
        assert!(error.contains("not found"), "{error}");
    }
}
