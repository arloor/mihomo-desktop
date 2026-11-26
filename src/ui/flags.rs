use gpui_kit::{Image, ImageFormat};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

static FLAG_IMAGE_CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<Image>>>>> = OnceLock::new();

/// Returns a cached full-color GPUI `Image` (rendered via resvg at 64x48 for crisp HiDPI)
/// for the given ISO 3166-1 alpha-2 country/region code.
pub fn flag_image(country_code: &str) -> Option<Arc<Image>> {
    let code = country_code.trim().to_ascii_uppercase();
    let cache = FLAG_IMAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(entry) = guard.get(&code) {
            return entry.clone();
        }
    }

    let img = flag_svg(&code)
        .map(|svg| Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())));

    if let Ok(mut guard) = cache.lock() {
        guard.insert(code, img.clone());
    }
    img
}

/// Human-friendly Chinese short name for common ISO 3166-1 alpha-2 codes.
pub fn country_name_zh(code: &str) -> Option<&'static str> {
    match code.trim().to_ascii_uppercase().as_str() {
        "US" => Some("美国"),
        "HK" => Some("香港"),
        "TW" => Some("台湾"),
        "JP" => Some("日本"),
        "SG" => Some("新加坡"),
        "KR" => Some("韩国"),
        "GB" | "UK" => Some("英国"),
        "DE" => Some("德国"),
        "FR" => Some("法国"),
        "NL" => Some("荷兰"),
        "CA" => Some("加拿大"),
        "AU" => Some("澳大利亚"),
        "CN" => Some("中国"),
        "MO" => Some("澳门"),
        "RU" => Some("俄罗斯"),
        "IN" => Some("印度"),
        "MY" => Some("马来西亚"),
        "TH" => Some("泰国"),
        "VN" => Some("越南"),
        "PH" => Some("菲律宾"),
        "ID" => Some("印度尼西亚"),
        "TR" => Some("土耳其"),
        "BR" => Some("巴西"),
        "AR" => Some("阿根廷"),
        "CL" => Some("智利"),
        "MX" => Some("墨西哥"),
        "CH" => Some("瑞士"),
        "SE" => Some("瑞典"),
        "NO" => Some("挪威"),
        "FI" => Some("芬兰"),
        "DK" => Some("丹麦"),
        "IE" => Some("爱尔兰"),
        "IT" => Some("意大利"),
        "ES" => Some("西班牙"),
        "PT" => Some("葡萄牙"),
        "PL" => Some("波兰"),
        "CZ" => Some("捷克"),
        "AT" => Some("奥地利"),
        "BE" => Some("比利时"),
        "LU" => Some("卢森堡"),
        "UA" => Some("乌克兰"),
        "RO" => Some("罗马尼亚"),
        "BG" => Some("保加利亚"),
        "HU" => Some("匈牙利"),
        "GR" => Some("希腊"),
        "IL" => Some("以色列"),
        "AE" => Some("阿联酋"),
        "SA" => Some("沙特"),
        "ZA" => Some("南非"),
        "NZ" => Some("新西兰"),
        "IS" => Some("冰岛"),
        "EE" => Some("爱沙尼亚"),
        "LV" => Some("拉脱维亚"),
        "LT" => Some("立陶宛"),
        "KZ" => Some("哈萨克斯坦"),
        "PK" => Some("巴基斯坦"),
        "BD" => Some("孟加拉"),
        "EG" => Some("埃及"),
        "NG" => Some("尼日利亚"),
        "CO" => Some("哥伦比亚"),
        "PE" => Some("秘鲁"),
        "SK" => Some("斯洛伐克"),
        "SI" => Some("斯洛文尼亚"),
        "HR" => Some("克罗地亚"),
        "RS" => Some("塞尔维亚"),
        "MT" => Some("马耳他"),
        "CY" => Some("塞浦路斯"),
        "KP" => Some("朝鲜"),
        "KH" => Some("柬埔寨"),
        "MM" => Some("缅甸"),
        "LA" => Some("老挝"),
        "MN" => Some("蒙古"),
        _ => None,
    }
}

fn wrap_svg(body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 48" width="64" height="48">{body}</svg>"#
    )
}

fn h_tri(top: &str, mid: &str, bot: &str) -> String {
    wrap_svg(&format!(
        r#"<rect width="64" height="16" fill="{top}"/><rect y="16" width="64" height="16" fill="{mid}"/><rect y="32" width="64" height="16" fill="{bot}"/>"#
    ))
}

fn v_tri(left: &str, mid: &str, right: &str) -> String {
    wrap_svg(&format!(
        r#"<rect width="21.4" height="48" fill="{left}"/><rect x="21.3" width="21.4" height="48" fill="{mid}"/><rect x="42.6" width="21.4" height="48" fill="{right}"/>"#
    ))
}

fn h_bi(top: &str, bot: &str) -> String {
    wrap_svg(&format!(
        r#"<rect width="64" height="24" fill="{top}"/><rect y="24" width="64" height="24" fill="{bot}"/>"#
    ))
}

fn nordic(bg: &str, cross: &str, outline: Option<&str>) -> String {
    let mut body = format!(r#"<rect width="64" height="48" fill="{bg}"/>"#);
    if let Some(out) = outline {
        body.push_str(&format!(
            r#"<rect x="17" width="12" height="48" fill="{out}"/><rect y="18" width="64" height="12" fill="{out}"/>"#
        ));
    }
    body.push_str(&format!(
        r#"<rect x="19.5" width="7" height="48" fill="{cross}"/><rect y="20.5" width="64" height="7" fill="{cross}"/>"#
    ));
    wrap_svg(&body)
}

/// Crisp 4:3 (64x48) full-color SVG definitions for major countries & proxy regions.
fn flag_svg(code: &str) -> Option<String> {
    let svg = match code {
        // United States
        "US" => wrap_svg(
            r##"<rect width="64" height="48" fill="#FFFFFF"/>
            <rect y="0" width="64" height="3.7" fill="#B22234"/>
            <rect y="7.4" width="64" height="3.7" fill="#B22234"/>
            <rect y="14.8" width="64" height="3.7" fill="#B22234"/>
            <rect y="22.2" width="64" height="3.7" fill="#B22234"/>
            <rect y="29.6" width="64" height="3.7" fill="#B22234"/>
            <rect y="37.0" width="64" height="3.7" fill="#B22234"/>
            <rect y="44.3" width="64" height="3.7" fill="#B22234"/>
            <rect width="27" height="25.9" fill="#3C3B6E"/>
            <g fill="#FFFFFF">
              <circle cx="4.5" cy="4.2" r="1.2"/><circle cx="10.5" cy="4.2" r="1.2"/><circle cx="16.5" cy="4.2" r="1.2"/><circle cx="22.5" cy="4.2" r="1.2"/>
              <circle cx="7.5" cy="8.4" r="1.2"/><circle cx="13.5" cy="8.4" r="1.2"/><circle cx="19.5" cy="8.4" r="1.2"/>
              <circle cx="4.5" cy="12.6" r="1.2"/><circle cx="10.5" cy="12.6" r="1.2"/><circle cx="16.5" cy="12.6" r="1.2"/><circle cx="22.5" cy="12.6" r="1.2"/>
              <circle cx="7.5" cy="16.8" r="1.2"/><circle cx="13.5" cy="16.8" r="1.2"/><circle cx="19.5" cy="16.8" r="1.2"/>
              <circle cx="4.5" cy="21.0" r="1.2"/><circle cx="10.5" cy="21.0" r="1.2"/><circle cx="16.5" cy="21.0" r="1.2"/><circle cx="22.5" cy="21.0" r="1.2"/>
            </g>"##,
        ),
        // China
        "CN" => wrap_svg(
            r##"<rect width="64" height="48" fill="#DE2910"/>
            <polygon fill="#FFDE00" points="11,6.5 12.6,11.4 17.8,11.4 13.6,14.4 15.2,19.3 11,16.3 6.8,19.3 8.4,14.4 4.2,11.4 9.4,11.4"/>
            <circle cx="22.5" cy="6.2" r="1.6" fill="#FFDE00"/>
            <circle cx="26.2" cy="10.4" r="1.6" fill="#FFDE00"/>
            <circle cx="26.2" cy="16.2" r="1.6" fill="#FFDE00"/>
            <circle cx="22.5" cy="20.4" r="1.6" fill="#FFDE00"/>"##,
        ),
        // Hong Kong
        "HK" => wrap_svg(
            r##"<rect width="64" height="48" fill="#DE2910"/>
            <g transform="translate(32,24)" fill="#FFFFFF">
              <ellipse cx="0" cy="-9" rx="3.8" ry="7.2" transform="rotate(0)"/>
              <ellipse cx="0" cy="-9" rx="3.8" ry="7.2" transform="rotate(72)"/>
              <ellipse cx="0" cy="-9" rx="3.8" ry="7.2" transform="rotate(144)"/>
              <ellipse cx="0" cy="-9" rx="3.8" ry="7.2" transform="rotate(216)"/>
              <ellipse cx="0" cy="-9" rx="3.8" ry="7.2" transform="rotate(288)"/>
              <circle cx="0" cy="0" r="2.2" fill="#DE2910"/>
            </g>"##,
        ),
        // Macau
        "MO" => wrap_svg(
            r##"<rect width="64" height="48" fill="#00785E"/>
            <circle cx="32" cy="27" r="7" fill="#FFFFFF"/>
            <ellipse cx="25" cy="29" rx="6" ry="3.5" fill="#FFFFFF"/>
            <ellipse cx="39" cy="29" rx="6" ry="3.5" fill="#FFFFFF"/>
            <rect x="20" y="34" width="24" height="2" fill="#FFFFFF"/>
            <rect x="23" y="38" width="18" height="2" fill="#FFFFFF"/>
            <circle cx="32" cy="11" r="2.6" fill="#FFDE00"/>
            <circle cx="23" cy="14" r="1.7" fill="#FFDE00"/>
            <circle cx="41" cy="14" r="1.7" fill="#FFDE00"/>
            <circle cx="17" cy="20" r="1.5" fill="#FFDE00"/>
            <circle cx="47" cy="20" r="1.5" fill="#FFDE00"/>"##,
        ),
        // Taiwan
        "TW" => wrap_svg(
            r##"<rect width="64" height="48" fill="#FE0000"/>
            <rect width="32" height="24" fill="#000095"/>
            <circle cx="16" cy="12" r="6.8" fill="#FFFFFF"/>
            <circle cx="16" cy="12" r="4.8" fill="#000095"/>
            <circle cx="16" cy="12" r="3.9" fill="#FFFFFF"/>"##,
        ),
        // Japan
        "JP" => wrap_svg(
            r##"<rect width="64" height="48" fill="#FFFFFF"/>
            <circle cx="32" cy="24" r="12.5" fill="#BC002D"/>"##,
        ),
        // Singapore
        "SG" => wrap_svg(
            r##"<rect width="64" height="24" fill="#EF3340"/>
            <rect y="24" width="64" height="24" fill="#FFFFFF"/>
            <circle cx="16.5" cy="12" r="7.5" fill="#FFFFFF"/>
            <circle cx="19.2" cy="12" r="6.6" fill="#EF3340"/>
            <g fill="#FFFFFF">
              <circle cx="21.5" cy="7.5" r="1.4"/>
              <circle cx="25.8" cy="10.6" r="1.4"/>
              <circle cx="24.2" cy="15.4" r="1.4"/>
              <circle cx="18.8" cy="15.4" r="1.4"/>
              <circle cx="17.2" cy="10.6" r="1.4"/>
            </g>"##,
        ),
        // South Korea
        "KR" => wrap_svg(
            r##"<rect width="64" height="48" fill="#FFFFFF"/>
            <circle cx="32" cy="24" r="10.5" fill="#CD2E3A"/>
            <path d="M21.5,24 A10.5,10.5 0 0,0 42.5,24 A5.25,5.25 0 0,1 32,24 A5.25,5.25 0 0,0 21.5,24 Z" fill="#0047A0"/>
            <g fill="#111111">
              <rect x="8" y="8" width="10" height="2.2" transform="rotate(-35 13 11)"/>
              <rect x="8" y="11.5" width="10" height="2.2" transform="rotate(-35 13 11)"/>
              <rect x="46" y="8" width="10" height="2.2" transform="rotate(35 51 11)"/>
              <rect x="46" y="11.5" width="10" height="2.2" transform="rotate(35 51 11)"/>
              <rect x="8" y="34.5" width="10" height="2.2" transform="rotate(35 13 37)"/>
              <rect x="8" y="38" width="10" height="2.2" transform="rotate(35 13 37)"/>
              <rect x="46" y="34.5" width="10" height="2.2" transform="rotate(-35 51 37)"/>
              <rect x="46" y="38" width="10" height="2.2" transform="rotate(-35 51 37)"/>
            </g>"##,
        ),
        // United Kingdom
        "GB" | "UK" => wrap_svg(
            r##"<rect width="64" height="48" fill="#012169"/>
            <path d="M0,0 L64,48 M64,0 L0,48" stroke="#FFFFFF" stroke-width="9"/>
            <path d="M0,0 L64,48 M64,0 L0,48" stroke="#C8102E" stroke-width="5"/>
            <path d="M32,0 V48 M0,24 H64" stroke="#FFFFFF" stroke-width="14"/>
            <path d="M32,0 V48 M0,24 H64" stroke="#C8102E" stroke-width="8"/>"##,
        ),
        // Canada
        "CA" => wrap_svg(
            r##"<rect width="64" height="48" fill="#FFFFFF"/>
            <rect width="16" height="48" fill="#D80621"/>
            <rect x="48" width="16" height="48" fill="#D80621"/>
            <polygon fill="#D80621" points="32,10 34.8,17.5 40.5,15.5 38,22.5 43,24.5 36.8,28.5 38.2,32.5 33,31.5 33,38 31,38 31,31.5 25.8,32.5 27.2,28.5 21,24.5 26,22.5 23.5,15.5 29.2,17.5"/>"##,
        ),
        // Australia
        "AU" => wrap_svg(
            r##"<rect width="64" height="48" fill="#00008B"/>
            <rect width="30" height="22" fill="#012169"/>
            <path d="M0,0 L30,22 M30,0 L0,22" stroke="#FFFFFF" stroke-width="4"/>
            <path d="M0,0 L30,22 M30,0 L0,22" stroke="#C8102E" stroke-width="2"/>
            <path d="M15,0 V22 M0,11 H30" stroke="#FFFFFF" stroke-width="6"/>
            <path d="M15,0 V22 M0,11 H30" stroke="#C8102E" stroke-width="3.5"/>
            <circle cx="15" cy="35" r="3.8" fill="#FFFFFF"/>
            <circle cx="48" cy="11" r="2.1" fill="#FFFFFF"/>
            <circle cx="42" cy="21" r="2.1" fill="#FFFFFF"/>
            <circle cx="55" cy="19" r="2.1" fill="#FFFFFF"/>
            <circle cx="48" cy="36" r="2.3" fill="#FFFFFF"/>
            <circle cx="52" cy="27" r="1.4" fill="#FFFFFF"/>"##,
        ),
        // New Zealand
        "NZ" => wrap_svg(
            r##"<rect width="64" height="48" fill="#00247D"/>
            <rect width="30" height="22" fill="#012169"/>
            <path d="M0,0 L30,22 M30,0 L0,22" stroke="#FFFFFF" stroke-width="4"/>
            <path d="M0,0 L30,22 M30,0 L0,22" stroke="#CC142B" stroke-width="2"/>
            <path d="M15,0 V22 M0,11 H30" stroke="#FFFFFF" stroke-width="6"/>
            <path d="M15,0 V22 M0,11 H30" stroke="#CC142B" stroke-width="3.5"/>
            <g fill="#CC142B" stroke="#FFFFFF" stroke-width="1">
              <circle cx="48" cy="11" r="2.3"/><circle cx="42" cy="22" r="2.1"/>
              <circle cx="55" cy="20" r="2.1"/><circle cx="48" cy="36" r="2.6"/>
            </g>"##,
        ),
        // Switzerland
        "CH" => wrap_svg(
            r##"<rect width="64" height="48" fill="#D52B1E"/>
            <rect x="27" y="10" width="10" height="28" fill="#FFFFFF"/>
            <rect x="18" y="19" width="28" height="10" fill="#FFFFFF"/>"##,
        ),
        // Turkey
        "TR" => wrap_svg(
            r##"<rect width="64" height="48" fill="#E30A17"/>
            <circle cx="25" cy="24" r="11.5" fill="#FFFFFF"/>
            <circle cx="28.2" cy="24" r="9.2" fill="#E30A17"/>
            <polygon fill="#FFFFFF" points="41,24 44.5,25.4 43.2,29 46.2,26.8 49.2,29 47.9,25.4 51,23.2 47.2,23.2 46.2,19.6 45,23.2"/>"##,
        ),
        // Brazil
        "BR" => wrap_svg(
            r##"<rect width="64" height="48" fill="#009C3B"/>
            <polygon points="32,6 58,24 32,42 6,24" fill="#FFDF00"/>
            <circle cx="32" cy="24" r="9.5" fill="#002776"/>
            <path d="M23,22 Q32,26 41,25" stroke="#FFFFFF" stroke-width="2" fill="none"/>"##,
        ),
        // Vietnam
        "VN" => wrap_svg(
            r##"<rect width="64" height="48" fill="#DA251D"/>
            <polygon fill="#FFFF00" points="32,11 35.3,21.1 46,21.1 37.3,27.4 40.6,37.5 32,31.2 23.4,37.5 26.7,27.4 18,21.1 28.7,21.1"/>"##,
        ),
        // Thailand
        "TH" => wrap_svg(
            r##"<rect width="64" height="48" fill="#A51931"/>
            <rect y="8" width="64" height="32" fill="#FFFFFF"/>
            <rect y="16" width="64" height="16" fill="#2D2A4A"/>"##,
        ),
        // Malaysia
        "MY" => wrap_svg(
            r##"<rect width="64" height="48" fill="#FFFFFF"/>
            <rect y="0" width="64" height="3.5" fill="#CC0001"/>
            <rect y="7" width="64" height="3.5" fill="#CC0001"/>
            <rect y="14" width="64" height="3.5" fill="#CC0001"/>
            <rect y="21" width="64" height="3.5" fill="#CC0001"/>
            <rect y="28" width="64" height="3.5" fill="#CC0001"/>
            <rect y="35" width="64" height="3.5" fill="#CC0001"/>
            <rect y="42" width="64" height="3.5" fill="#CC0001"/>
            <rect width="32" height="24.5" fill="#010066"/>
            <circle cx="13.5" cy="12.2" r="7" fill="#FFCC00"/>
            <circle cx="15.8" cy="12.2" r="5.8" fill="#010066"/>
            <circle cx="22.5" cy="12.2" r="3.6" fill="#FFCC00"/>"##,
        ),
        // Philippines
        "PH" => wrap_svg(
            r##"<rect width="64" height="24" fill="#0038A8"/>
            <rect y="24" width="64" height="24" fill="#CE1126"/>
            <polygon points="0,0 28,24 0,48" fill="#FFFFFF"/>
            <circle cx="10.5" cy="24" r="4.5" fill="#FCD116"/>"##,
        ),
        // India
        "IN" => wrap_svg(
            r##"<rect width="64" height="16" fill="#FF9933"/>
            <rect y="16" width="64" height="16" fill="#FFFFFF"/>
            <rect y="32" width="64" height="16" fill="#138808"/>
            <circle cx="32" cy="24" r="5.5" fill="none" stroke="#000080" stroke-width="1.6"/>
            <circle cx="32" cy="24" r="1.5" fill="#000080"/>"##,
        ),
        // Czechia
        "CZ" => wrap_svg(
            r##"<rect width="64" height="24" fill="#FFFFFF"/>
            <rect y="24" width="64" height="24" fill="#D7141A"/>
            <polygon points="0,0 30,24 0,48" fill="#11457E"/>"##,
        ),
        // Spain
        "ES" => wrap_svg(
            r##"<rect width="64" height="48" fill="#AA151B"/>
            <rect y="12" width="64" height="24" fill="#F1BF00"/>
            <rect x="14" y="18" width="8" height="12" rx="2" fill="#AA151B"/>"##,
        ),
        // Portugal
        "PT" => wrap_svg(
            r##"<rect width="26" height="48" fill="#006600"/>
            <rect x="26" width="38" height="48" fill="#FF0000"/>
            <circle cx="26" cy="24" r="8" fill="#FFFF00"/>
            <circle cx="26" cy="24" r="5" fill="#FF0000"/>"##,
        ),
        // Greece
        "GR" => wrap_svg(
            r##"<rect width="64" height="48" fill="#0D5EAF"/>
            <rect y="5.3" width="64" height="5.3" fill="#FFFFFF"/>
            <rect y="16" width="64" height="5.3" fill="#FFFFFF"/>
            <rect y="26.7" width="64" height="5.3" fill="#FFFFFF"/>
            <rect y="37.3" width="64" height="5.3" fill="#FFFFFF"/>
            <rect width="26" height="26.7" fill="#0D5EAF"/>
            <rect x="10.5" width="5" height="26.7" fill="#FFFFFF"/>
            <rect y="10.8" width="26" height="5" fill="#FFFFFF"/>"##,
        ),
        // Israel
        "IL" => wrap_svg(
            r##"<rect width="64" height="48" fill="#FFFFFF"/>
            <rect y="5" width="64" height="6" fill="#0038B8"/>
            <rect y="37" width="64" height="6" fill="#0038B8"/>
            <polygon points="32,14 40,28 24,28" fill="none" stroke="#0038B8" stroke-width="2"/>
            <polygon points="32,34 40,20 24,20" fill="none" stroke="#0038B8" stroke-width="2"/>"##,
        ),
        // United Arab Emirates
        "AE" => wrap_svg(
            r##"<rect width="64" height="16" fill="#00732F"/>
            <rect y="16" width="64" height="16" fill="#FFFFFF"/>
            <rect y="32" width="64" height="16" fill="#000000"/>
            <rect width="16" height="48" fill="#FF0000"/>"##,
        ),
        // Saudi Arabia
        "SA" => wrap_svg(
            r##"<rect width="64" height="48" fill="#006C35"/>
            <rect x="14" y="16" width="36" height="6" rx="2" fill="#FFFFFF"/>
            <rect x="16" y="29" width="32" height="2.8" rx="1" fill="#FFFFFF"/>"##,
        ),
        // South Africa
        "ZA" => wrap_svg(
            r##"<rect width="64" height="24" fill="#E03C31"/>
            <rect y="24" width="64" height="24" fill="#001489"/>
            <path d="M0,0 L28,24 L0,48 M28,24 H64" stroke="#FFFFFF" stroke-width="14" fill="none"/>
            <path d="M0,0 L28,24 L0,48 M28,24 H64" stroke="#007749" stroke-width="9" fill="none"/>
            <polygon points="0,8 19,24 0,40" fill="#FFB81C"/>
            <polygon points="0,12 14,24 0,36" fill="#000000"/>"##,
        ),
        // Argentina
        "AR" => wrap_svg(
            r##"<rect width="64" height="16" fill="#74ACDF"/>
            <rect y="16" width="64" height="16" fill="#FFFFFF"/>
            <rect y="32" width="64" height="16" fill="#74ACDF"/>
            <circle cx="32" cy="24" r="4.5" fill="#F6B40E"/>"##,
        ),
        // Chile
        "CL" => wrap_svg(
            r##"<rect width="64" height="24" fill="#FFFFFF"/>
            <rect y="24" width="64" height="24" fill="#D52B1E"/>
            <rect width="24" height="24" fill="#0039A6"/>
            <polygon fill="#FFFFFF" points="12,5 13.8,10.5 19.5,10.5 14.9,13.8 16.6,19.2 12,15.9 7.4,19.2 9.1,13.8 4.5,10.5 10.2,10.5"/>"##,
        ),
        // Mexico
        "MX" => wrap_svg(
            r##"<rect width="21.4" height="48" fill="#006847"/>
            <rect x="21.3" width="21.4" height="48" fill="#FFFFFF"/>
            <rect x="42.6" width="21.4" height="48" fill="#CE1126"/>
            <circle cx="32" cy="24" r="5" fill="#8C5A2B"/>"##,
        ),
        // Pakistan
        "PK" => wrap_svg(
            r##"<rect width="16" height="48" fill="#FFFFFF"/>
            <rect x="16" width="48" height="48" fill="#01411C"/>
            <circle cx="40" cy="24" r="10.5" fill="#FFFFFF"/>
            <circle cx="43.5" cy="22" r="9" fill="#01411C"/>
            <circle cx="45" cy="19" r="2.2" fill="#FFFFFF"/>"##,
        ),
        // Bangladesh
        "BD" => wrap_svg(
            r##"<rect width="64" height="48" fill="#006A4E"/>
            <circle cx="28" cy="24" r="12" fill="#F42A41"/>"##,
        ),
        // Kazakhstan
        "KZ" => wrap_svg(
            r##"<rect width="64" height="48" fill="#00AFCA"/>
            <circle cx="32" cy="22" r="8" fill="#FEC50C"/>
            <rect x="4" y="6" width="4" height="36" fill="#FEC50C"/>"##,
        ),
        // Horizontal tricolors
        "DE" => h_tri("#000000", "#DD0000", "#FFCE00"),
        "NL" => h_tri("#AE1C28", "#FFFFFF", "#21468B"),
        "RU" => h_tri("#FFFFFF", "#0039A6", "#D52B1E"),
        "AT" => h_tri("#ED2939", "#FFFFFF", "#ED2939"),
        "LU" => h_tri("#ED2939", "#FFFFFF", "#00A1DE"),
        "BG" => h_tri("#FFFFFF", "#00966E", "#D62612"),
        "HU" => h_tri("#CE2939", "#FFFFFF", "#477050"),
        "EE" => h_tri("#0072CE", "#000000", "#FFFFFF"),
        "LV" => wrap_svg(
            r##"<rect width="64" height="48" fill="#9E3039"/><rect y="19" width="64" height="10" fill="#FFFFFF"/>"##,
        ),
        "LT" => h_tri("#FDB913", "#006A44", "#C1272D"),
        "EG" => h_tri("#CE1126", "#FFFFFF", "#000000"),
        "CO" => wrap_svg(
            r##"<rect width="64" height="24" fill="#FCD116"/><rect y="24" width="64" height="12" fill="#003893"/><rect y="36" width="64" height="12" fill="#CE1126"/>"##,
        ),
        "SK" => h_tri("#FFFFFF", "#0B4EA2", "#EE1C25"),
        "SI" => h_tri("#FFFFFF", "#005DA4", "#ED1C24"),
        "HR" => h_tri("#FF0000", "#FFFFFF", "#171796"),
        "RS" => h_tri("#C6363C", "#0C4076", "#FFFFFF"),
        // Horizontal bicolors
        "UA" => h_bi("#005BBB", "#FFD500"),
        "PL" => h_bi("#FFFFFF", "#DC143C"),
        "ID" => h_bi("#CE1126", "#FFFFFF"),
        // Vertical tricolors
        "FR" => v_tri("#002395", "#FFFFFF", "#ED2939"),
        "IT" => v_tri("#009246", "#FFFFFF", "#CE2B37"),
        "IE" => v_tri("#169B62", "#FFFFFF", "#FF883E"),
        "BE" => v_tri("#000000", "#FAE042", "#ED2939"),
        "RO" => v_tri("#002B7F", "#FCD116", "#CE1126"),
        "NG" => v_tri("#008751", "#FFFFFF", "#008751"),
        "PE" => v_tri("#D91023", "#FFFFFF", "#D91023"),
        // Nordic crosses
        "SE" => nordic("#006AA7", "#FECC00", None),
        "DK" => nordic("#C60C30", "#FFFFFF", None),
        "FI" => nordic("#FFFFFF", "#003580", None),
        "NO" => nordic("#BA0C2F", "#00205B", Some("#FFFFFF")),
        "IS" => nordic("#02529C", "#DC1E35", Some("#FFFFFF")),
        // Malta / Cyprus
        "MT" => wrap_svg(
            r##"<rect width="32" height="48" fill="#FFFFFF"/><rect x="32" width="32" height="48" fill="#CF142B"/>"##,
        ),
        "CY" => wrap_svg(
            r##"<rect width="64" height="48" fill="#FFFFFF"/><ellipse cx="32" cy="24" rx="14" ry="8" fill="#D57800"/>"##,
        ),
        _ => return None,
    };
    Some(svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_flag_images_for_major_regions() {
        for code in ["US", "HK", "TW", "JP", "SG", "KR", "GB", "DE", "FR", "CN", "NL", "CA", "AU"] {
            let img = flag_image(code).expect("flag image should exist");
            let bytes = img.bytes();
            let text = std::str::from_utf8(bytes).expect("svg should be utf-8");
            assert!(text.starts_with("<svg ") && text.ends_with("</svg>"));
            assert!(country_name_zh(code).is_some());
        }
        assert!(flag_image("ZZ").is_none());
    }
}
