/* Khu vực hành chính: 34 tỉnh thành Việt Nam (sau sáp nhập 1/7/2025), nhận diện tỉnh/quốc gia cho mỗi điểm. */
const Geo = (() => {
  // [tên, miền]
  const UNITS = [
    ['Hà Nội', 'Bắc'], ['Hải Phòng', 'Bắc'], ['Quảng Ninh', 'Bắc'], ['Bắc Ninh', 'Bắc'], ['Hưng Yên', 'Bắc'],
    ['Ninh Bình', 'Bắc'], ['Phú Thọ', 'Bắc'], ['Thái Nguyên', 'Bắc'], ['Tuyên Quang', 'Bắc'], ['Lào Cai', 'Bắc'],
    ['Lai Châu', 'Bắc'], ['Điện Biên', 'Bắc'], ['Sơn La', 'Bắc'], ['Lạng Sơn', 'Bắc'], ['Cao Bằng', 'Bắc'],
    ['Thanh Hóa', 'Trung'], ['Nghệ An', 'Trung'], ['Hà Tĩnh', 'Trung'], ['Quảng Trị', 'Trung'], ['Huế', 'Trung'],
    ['Đà Nẵng', 'Trung'], ['Quảng Ngãi', 'Trung'], ['Gia Lai', 'Trung'], ['Đắk Lắk', 'Trung'], ['Khánh Hòa', 'Trung'],
    ['Lâm Đồng', 'Trung'],
    ['TP. Hồ Chí Minh', 'Nam'], ['Đồng Nai', 'Nam'], ['Tây Ninh', 'Nam'], ['Đồng Tháp', 'Nam'], ['Vĩnh Long', 'Nam'],
    ['Cần Thơ', 'Nam'], ['An Giang', 'Nam'], ['Cà Mau', 'Nam']
  ].map(([name, region]) => ({ name, region }));

  // Tên tỉnh cũ (và cách viết khác) → đơn vị hiện nay
  const MERGED = {
    'Tuyên Quang': ['Hà Giang'],
    'Lào Cai': ['Yên Bái'],
    'Thái Nguyên': ['Bắc Kạn'],
    'Phú Thọ': ['Vĩnh Phúc', 'Hòa Bình'],
    'Bắc Ninh': ['Bắc Giang'],
    'Hưng Yên': ['Thái Bình'],
    'Hải Phòng': ['Hải Dương', 'Haiphong'],
    'Ninh Bình': ['Hà Nam', 'Nam Định'],
    'Quảng Trị': ['Quảng Bình'],
    'Đà Nẵng': ['Quảng Nam'],
    'Quảng Ngãi': ['Kon Tum'],
    'Gia Lai': ['Bình Định'],
    'Khánh Hòa': ['Ninh Thuận'],
    'Lâm Đồng': ['Đắk Nông', 'Bình Thuận'],
    'Đắk Lắk': ['Phú Yên', 'Dak Lak'],
    'TP. Hồ Chí Minh': ['Hồ Chí Minh', 'Bình Dương', 'Bà Rịa Vũng Tàu', 'Sài Gòn', 'Saigon'],
    'Đồng Nai': ['Bình Phước'],
    'Tây Ninh': ['Long An'],
    'Cần Thơ': ['Sóc Trăng', 'Hậu Giang'],
    'Vĩnh Long': ['Bến Tre', 'Trà Vinh'],
    'Đồng Tháp': ['Tiền Giang'],
    'Cà Mau': ['Bạc Liêu'],
    'An Giang': ['Kiên Giang'],
    'Hà Nội': ['Hanoi'],
    'Huế': ['Thừa Thiên Huế', 'Thua Thien-Hue']
  };

  // Mốc gần đúng (tỉnh lỵ cũ) để đoán tỉnh khi không có mạng: [vĩ độ, kinh độ, đơn vị hiện nay]
  const ANCHORS = [
    [21.03, 105.85, 'Hà Nội'], [22.82, 104.98, 'Tuyên Quang'], [22.67, 106.26, 'Cao Bằng'], [22.15, 105.83, 'Thái Nguyên'],
    [21.82, 105.21, 'Tuyên Quang'], [22.49, 103.97, 'Lào Cai'], [22.34, 103.84, 'Lào Cai'], [21.39, 103.02, 'Điện Biên'],
    [22.40, 103.46, 'Lai Châu'], [21.33, 103.91, 'Sơn La'], [20.85, 104.64, 'Sơn La'], [21.72, 104.90, 'Lào Cai'],
    [20.82, 105.34, 'Phú Thọ'], [21.59, 105.84, 'Thái Nguyên'], [21.85, 106.76, 'Lạng Sơn'], [20.96, 107.05, 'Quảng Ninh'],
    [21.27, 106.19, 'Bắc Ninh'], [21.32, 105.40, 'Phú Thọ'], [21.31, 105.60, 'Phú Thọ'], [21.19, 106.07, 'Bắc Ninh'],
    [20.94, 106.33, 'Hải Phòng'], [20.86, 106.68, 'Hải Phòng'], [20.65, 106.05, 'Hưng Yên'], [20.45, 106.34, 'Hưng Yên'],
    [20.54, 105.91, 'Ninh Bình'], [20.42, 106.17, 'Ninh Bình'], [20.25, 105.97, 'Ninh Bình'], [19.81, 105.78, 'Thanh Hóa'],
    [18.68, 105.68, 'Nghệ An'], [19.25, 104.90, 'Nghệ An'], [18.34, 105.90, 'Hà Tĩnh'], [17.47, 106.60, 'Quảng Trị'],
    [16.82, 107.10, 'Quảng Trị'], [16.46, 107.59, 'Huế'], [16.05, 108.21, 'Đà Nẵng'], [15.88, 108.33, 'Đà Nẵng'],
    [15.57, 108.47, 'Đà Nẵng'], [15.12, 108.80, 'Quảng Ngãi'], [13.78, 109.22, 'Gia Lai'], [13.09, 109.30, 'Đắk Lắk'],
    [12.24, 109.19, 'Khánh Hòa'], [11.57, 108.99, 'Khánh Hòa'], [10.93, 108.10, 'Lâm Đồng'], [14.35, 108.00, 'Quảng Ngãi'],
    [13.98, 108.00, 'Gia Lai'], [12.67, 108.04, 'Đắk Lắk'], [12.00, 107.69, 'Lâm Đồng'], [11.94, 108.44, 'Lâm Đồng'],
    [11.55, 107.81, 'Lâm Đồng'], [11.53, 106.88, 'Đồng Nai'], [11.31, 106.10, 'Tây Ninh'], [10.98, 106.65, 'TP. Hồ Chí Minh'],
    [10.95, 106.82, 'Đồng Nai'], [10.50, 107.17, 'TP. Hồ Chí Minh'], [10.35, 107.08, 'TP. Hồ Chí Minh'], [10.78, 106.70, 'TP. Hồ Chí Minh'],
    [8.68, 106.61, 'TP. Hồ Chí Minh'], [10.54, 106.41, 'Tây Ninh'], [10.36, 106.36, 'Đồng Tháp'], [10.24, 106.38, 'Vĩnh Long'],
    [9.93, 106.34, 'Vĩnh Long'], [10.25, 105.97, 'Vĩnh Long'], [10.46, 105.63, 'Đồng Tháp'], [10.39, 105.44, 'An Giang'],
    [10.01, 105.08, 'An Giang'], [10.22, 103.96, 'An Giang'], [10.03, 105.77, 'Cần Thơ'], [9.78, 105.47, 'Cần Thơ'],
    [9.60, 105.97, 'Cần Thơ'], [9.29, 105.72, 'Cà Mau'], [9.18, 105.15, 'Cà Mau']
  ];

  const VN_BOUNDS = [[8.2, 101.9], [23.6, 110.1]];

  const norm = s => String(s || '').toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/đ/g, 'd')
    .replace(/[^a-z0-9 ]+/g, ' ')
    .replace(/\b(tinh|thanh pho|tp|province|city|municipality)\b/g, ' ')
    .replace(/\s+/g, ' ').trim();

  const LOOKUP = {};
  UNITS.forEach(u => { LOOKUP[norm(u.name)] = u.name; });
  Object.entries(MERGED).forEach(([unit, olds]) => olds.forEach(o => { LOOKUP[norm(o)] = unit.trim(); }));

  const vnUnit = name => LOOKUP[norm(name)] || null;

  function hav(a, b) {
    const R = 6371, toR = Math.PI / 180;
    const dLat = (b[0] - a[0]) * toR, dLng = (b[1] - a[1]) * toR;
    const s = Math.sin(dLat / 2) ** 2 + Math.cos(a[0] * toR) * Math.cos(b[0] * toR) * Math.sin(dLng / 2) ** 2;
    return 2 * R * Math.asin(Math.sqrt(s));
  }

  function inPolygon(lat, lng, poly) {
    let inside = false;
    for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
      const [yi, xi] = poly[i], [yj, xj] = poly[j];
      if ((yi > lat) !== (yj > lat) && lng < ((xj - xi) * (lat - yi)) / (yj - yi) + xi) inside = !inside;
    }
    return inside;
  }
  const inVietnam = (lat, lng) => typeof VN_SHAPE !== 'undefined' && VN_SHAPE.some(p => inPolygon(lat, lng, p));

  function nearest(lat, lng) {
    let best = null, bd = Infinity;
    for (const a of ANCHORS) {
      const d = hav([lat, lng], a);
      if (d < bd) { bd = d; best = a; }
    }
    return { unit: best[2], km: bd };
  }

  /** Đoán khu vực không cần mạng. Trả về null nếu điểm không thuộc Việt Nam. */
  function guess(lat, lng) {
    if (lat < VN_BOUNDS[0][0] || lat > VN_BOUNDS[1][0] || lng < VN_BOUNDS[0][1] || lng > VN_BOUNDS[1][1]) return null;
    const n = nearest(lat, lng);
    if (!inVietnam(lat, lng) && n.km > 35) return null;
    return { province: n.unit, country: 'Việt Nam', cc: 'VN', src: 'guess' };
  }

  /** Đọc kết quả Nominatim (reverse) thành { province, country, cc }. */
  function fromAddress(a, lat, lng) {
    if (!a) return null;
    const cc = (a.country_code || '').toUpperCase();
    if (!cc) return null;
    if (cc === 'VN') {
      const cands = [a.state, a.province, a.city, a.region, a.county, a.municipality, a.town];
      let unit = null;
      for (const c of cands) { if (c && (unit = vnUnit(c))) break; }
      if (!unit) unit = nearest(lat, lng).unit;
      return { province: unit, country: 'Việt Nam', cc, src: 'geo' };
    }
    const clean = s => String(s || '').replace(/\b(Tỉnh|Thành phố|Bang|Province|Prefecture)\b/gi, '').trim();
    return { province: clean(a.state || a.province || a.region || a.city || a.county || ''), country: a.country || cc, cc, src: 'geo' };
  }

  const flag = cc => (cc && cc.length === 2 ? String.fromCodePoint(...[...cc.toUpperCase()].map(c => 0x1F1E6 + c.charCodeAt(0) - 65)) : '🌏');

  return { UNITS, VN_BOUNDS, vnUnit, guess, fromAddress, inVietnam, flag, norm };
})();
