/* Dữ liệu mẫu: vài chuyến đi trải dọc chữ S và một chuyến nước ngoài.
   Mọi bản ghi đều có demo:true để có thể xoá riêng. Ảnh là hình minh hoạ vẽ bằng canvas. */
const Demo = (() => {
  const at = (iso, h, m = 0) => new Date(`${iso}T${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:00+07:00`).getTime();
  const VN = { country: 'Việt Nam', cc: 'VN' };

  // p: [c1, c2, emoji] cho từng ảnh minh hoạ
  const TRIPS = [
    {
      name: 'Bangkok cuối tuần', note: '', region: { province: 'Bangkok', country: 'Thái Lan', cc: 'TH' }, route: false, stops: [
        { name: 'Wat Arun', cat: 'sight', lat: 13.7437, lng: 100.4889, t: ['2026-08-15', 9, 20], rating: 5, price: 1, mood: 5, tags: ['Chụp ảnh đẹp', 'View đẹp'], feedback: 'Nên đi sáng sớm cho đỡ nắng, thuê đồ Thái ngay cổng. Leo bậc thang hơi dốc.', memory: 'Hoàng hôn bên kia sông nhìn sang chùa đẹp muốn khóc.', p: [['#F6C8B8', '#F8D9E2', '🛕']] },
        { name: 'Chợ Chatuchak', cat: 'shop', lat: 13.7999, lng: 100.5503, t: ['2026-08-16', 10, 40], rating: 4, price: 1, mood: 4, tags: ['Đông đúc', 'Giá hợp lý'], feedback: 'Rất rộng, nên lưu lại số khu để quay lại. Kem dừa ở khu 4 ngon.', memory: '', p: [['#F8D9E2', '#F3E8DA', '🥥']] }
      ]
    },
    {
      name: 'Sài Gòn một chiều', note: '', region: { province: 'TP. Hồ Chí Minh', ...VN }, route: false, stops: [
        { name: 'Bưu điện Trung tâm Sài Gòn', cat: 'sight', lat: 10.7799, lng: 106.6999, t: ['2026-09-12', 15, 0], rating: 4, price: 0, mood: 3, tags: ['Chụp ảnh đẹp'], feedback: 'Vào cửa miễn phí, có thể gửi bưu thiếp ngay tại quầy.', memory: 'Gửi một tấm thiệp về nhà.', p: [['#F3D9B8', '#F8D9E2', '📮']] },
        { name: 'Cà phê chung cư Nguyễn Huệ', cat: 'cafe', lat: 10.7741, lng: 106.7040, t: ['2026-09-12', 16, 30], rating: 4, price: 2, mood: 4, tags: ['Không gian xinh', 'View đẹp', 'Đông đúc'], feedback: 'Ban công nhìn xuống phố đi bộ. Thang máy tính phí 3k, đi thang bộ cũng vui.', memory: '', p: [] }
      ]
    },
    {
      name: 'Hội An một ngày nắng', note: 'Đạp xe từ phố cổ ra biển, nắng vàng như mật.', region: { province: 'Đà Nẵng', ...VN }, route: true, speed: 3.2, stops: [
        { name: 'Chùa Cầu', cat: 'sight', lat: 15.8771, lng: 108.3260, t: ['2026-09-26', 8, 10], rating: 5, price: 1, mood: 4, tags: ['Chụp ảnh đẹp', 'Đông đúc'], feedback: 'Sau 9 giờ rất đông khách đoàn. Vé tham quan phố cổ dùng được cho 5 điểm.', memory: 'Phố còn vắng, đèn lồng chưa tắt hết.', p: [['#F5D08A', '#F8D9E2', '🏮'], ['#F8D9E2', '#F3E8DA', '🌉']] },
        { name: 'Cơm gà phố Hội', cat: 'food', lat: 15.8776, lng: 108.3282, t: ['2026-09-26', 11, 15], rating: 5, price: 1, mood: 5, tags: ['Đồ ăn ngon', 'Đáng quay lại', 'Giá hợp lý'], feedback: 'Cơm vàng nghệ, gà xé trộn rau răm, thêm chén lòng mề. 45k một phần.', memory: '', p: [['#F6DFA6', '#FBE3EA', '🍗']] },
        { name: 'Bãi biển An Bàng', cat: 'nature', lat: 15.9140, lng: 108.3410, t: ['2026-09-26', 15, 30], rating: 4, price: 1, mood: 3, tags: ['View đẹp', 'Yên tĩnh'], feedback: 'Gửi xe 10k, thuê ghế kèm nước dừa. Sóng êm, nước trong.', memory: 'Ngủ quên trên ghế, tỉnh dậy thấy trời đã tím.', p: [['#BFE3EA', '#F8D9E2', '🏖️']] }
      ]
    },
    {
      name: 'Sáng chủ nhật ở phố cổ', note: 'Một buổi sáng đi bộ chậm rãi, ăn sáng hai lần.', region: { province: 'Hà Nội', ...VN }, route: true, speed: 1.2, stops: [
        { name: 'Hồ Hoàn Kiếm', cat: 'nature', lat: 21.0287, lng: 105.8524, t: ['2026-10-19', 6, 30], rating: 5, price: 0, mood: 3, tags: ['View đẹp'], feedback: 'Sáng sớm nhiều người tập thể dục, không khí trong lành.', memory: 'Cầu Thê Húc đỏ rực trong nắng sớm.', p: [['#F4B3C5', '#F3E8DA', '🌅']] },
        { name: 'Cà phê trứng phố cổ', cat: 'cafe', lat: 21.0331, lng: 105.8541, t: ['2026-10-19', 7, 45], rating: 5, price: 1, mood: 5, tags: ['Đồ ăn ngon', 'Đáng quay lại', 'Khó tìm'], feedback: 'Lối vào nằm sâu trong hẻm, lên tầng 3 có ban công nhìn ra hồ. Trứng đánh bông mịn, không tanh.', memory: '', p: [['#E9D3C0', '#FFF3E6', '☕']] },
        { name: 'Bún chả Hàng Mành', cat: 'food', lat: 21.0335, lng: 105.8490, t: ['2026-10-19', 11, 30], rating: 4, price: 1, mood: 4, tags: ['Đồ ăn ngon', 'Đông đúc'], feedback: 'Chả nướng thơm than hoa, nước chấm vừa miệng. Giờ trưa rất đông.', memory: '', p: [] }
      ]
    },
    {
      name: 'Đà Lạt ngày 1 · Phố & hồ', note: 'Ngày đầu tiên lạnh hơn tưởng tượng, nhưng mọi thứ đều dễ thương.', region: { province: 'Lâm Đồng', ...VN }, route: true, speed: 5.5, stops: [
        { name: 'Chợ Đà Lạt', cat: 'shop', lat: 11.9429, lng: 108.4372, t: ['2026-11-07', 8, 40], rating: 4, price: 1, mood: 4, tags: ['Đông đúc', 'Giá hợp lý'], feedback: 'Sáng sớm chợ còn mát, dâu tây và mứt rất nhiều lựa chọn. Nhớ trả giá nhẹ nhàng.', memory: 'Mua túi dâu đầu tiên, ăn luôn ngoài hiên chợ.', p: [['#F7C9D6', '#F3E8DA', '🍓']] },
        { name: 'Quán bánh căn Nhà Chung', cat: 'food', lat: 11.9381, lng: 108.4369, t: ['2026-11-07', 9, 30], rating: 5, price: 1, mood: 5, tags: ['Đồ ăn ngon', 'Đáng quay lại', 'Giá hợp lý'], feedback: 'Bánh căn trứng cút giòn, nước chấm xíu mại đậm vị. Phải xếp hàng khoảng 15 phút.', memory: 'Ngồi ghế nhựa thấp, gió lạnh mà bánh nóng hổi.', p: [['#F4B3C5', '#FFF3E6', '🥞'], ['#EED6C4', '#FBE3EA', '🍳']] },
        { name: 'Hồ Xuân Hương', cat: 'nature', lat: 11.9404, lng: 108.4446, t: ['2026-11-07', 10, 45], rating: 5, price: 0, mood: 3, tags: ['View đẹp', 'Chụp ảnh đẹp'], feedback: 'Đi bộ một vòng hồ mất chừng 1 tiếng, có xe đạp đôi cho thuê.', memory: 'Sương tan dần trên mặt hồ, cả hai im lặng nhìn rất lâu.', p: [['#CFE3E8', '#F8D9E2', '🦢'], ['#F3E8DA', '#D9E8EC', '🌫️'], ['#F8D9E2', '#E4D2BE', '🚲']] },
        { name: 'Tiệm cà phê trên đồi', cat: 'cafe', lat: 11.9548, lng: 108.4318, t: ['2026-11-07', 15, 10], rating: 4, price: 2, mood: 5, tags: ['Không gian xinh', 'View đẹp', 'Hơi đắt'], feedback: 'Cà phê muối ổn, view đồi thông rất chill. Cuối tuần hơi đông, nên đến trước 3 giờ.', memory: 'Viết tấm postcard gửi cho chính mình năm sau.', p: [['#E9D3C0', '#F8D9E2', '☕'], ['#F4B3C5', '#F3E8DA', '🌲']] },
        { name: 'Quán lẩu gà lá é', cat: 'food', lat: 11.9461, lng: 108.4331, t: ['2026-11-07', 19, 0], rating: 4, price: 2, mood: 4, tags: ['Đồ ăn ngon', 'Phục vụ tốt'], feedback: 'Lẩu thơm lá é, gà ta chắc thịt. Nồi nhỏ đủ cho 2–3 người.', memory: 'Trời 14 độ, ăn lẩu là đúng bài.', p: [['#F7C9D6', '#EED6C4', '🍲']] }
      ]
    },
    {
      name: 'Đà Lạt ngày 2 · Ga & thung lũng', note: 'Đi xe máy loanh quanh, chiều về trời đổ mưa nhẹ.', region: { province: 'Lâm Đồng', ...VN }, route: true, speed: 7, stops: [
        { name: 'Nhà thờ Con Gà', cat: 'sight', lat: 11.9363, lng: 108.4380, t: ['2026-11-08', 8, 15], rating: 4, price: 0, mood: 3, tags: ['Chụp ảnh đẹp'], feedback: 'Kiến trúc Pháp màu hồng nhạt, vào buổi sáng ít người.', memory: '', p: [['#F8D9E2', '#FFF3E6', '⛪']] },
        { name: 'Ga Đà Lạt', cat: 'sight', lat: 11.9416, lng: 108.4555, t: ['2026-11-08', 9, 40], rating: 5, price: 1, mood: 5, tags: ['Chụp ảnh đẹp', 'View đẹp', 'Đáng quay lại'], feedback: 'Vé tham quan rẻ, có tàu chạy tuyến Trại Mát. Toa tàu gỗ cũ rất đẹp để chụp.', memory: 'Tiếng còi tàu làm cả hai giật mình rồi bật cười.', p: [['#E4D2BE', '#F4B3C5', '🚂'], ['#F3E8DA', '#F8D9E2', '🎫']] },
        { name: 'Quán bánh căn Nhà Chung', cat: 'food', lat: 11.9381, lng: 108.4369, t: ['2026-11-08', 11, 30], rating: 5, price: 1, mood: 5, tags: ['Đồ ăn ngon', 'Đáng quay lại'], feedback: 'Quay lại lần hai, lần này gọi thêm phần xíu mại riêng.', memory: '', p: [] },
        { name: 'Thung lũng Tình Yêu', cat: 'fun', lat: 11.9780, lng: 108.4500, t: ['2026-11-08', 14, 20], rating: 3, price: 3, mood: 3, tags: ['Hơi đắt', 'Chụp ảnh đẹp'], feedback: 'Cảnh đẹp nhưng nhiều tiểu cảnh thương mại. Vé hơi cao so với trải nghiệm.', memory: 'Hái được một bông hoa dại kẹp vào sổ.', p: [['#F8D9E2', '#E1EBD9', '🌸']] }
      ]
    }
  ];

  function hav(a, b) {
    const R = 6371000, toR = Math.PI / 180;
    const dLat = (b[0] - a[0]) * toR, dLng = (b[1] - a[1]) * toR;
    const s = Math.sin(dLat / 2) ** 2 + Math.cos(a[0] * toR) * Math.cos(b[0] * toR) * Math.sin(dLng / 2) ** 2;
    return 2 * R * Math.asin(Math.sqrt(s));
  }

  // Nội suy giữa các điểm dừng, thêm độ cong nhẹ cho giống đường phố.
  function buildRoute(stops, speed) {
    const pts = [];
    for (let i = 0; i < stops.length - 1; i++) {
      const a = stops[i], b = stops[i + 1];
      const d = hav([a.lat, a.lng], [b.lat, b.lng]);
      const n = Math.max(4, Math.round(d / 35));
      const t0 = a.time + 15 * 60000;
      const travel = Math.min(d / speed * 1000, b.time - t0 - 5 * 60000);
      const bend = (i % 2 ? 1 : -1) * Math.min(0.0012, d / 900000);
      for (let k = 0; k <= n; k++) {
        const f = k / n;
        const curve = Math.sin(f * Math.PI) * bend;
        const wob = Math.sin(f * Math.PI * 6) * 0.00006;
        pts.push([+(a.lat + (b.lat - a.lat) * f + curve + wob).toFixed(6), +(a.lng + (b.lng - a.lng) * f - curve * 0.6).toFixed(6), Math.round(t0 + travel * f), 0]);
      }
    }
    return pts;
  }

  function drawPhoto(c1, c2, emoji, w, h) {
    const cv = document.createElement('canvas');
    cv.width = w; cv.height = h;
    const g = cv.getContext('2d');
    const grd = g.createLinearGradient(0, 0, w, h);
    grd.addColorStop(0, c1); grd.addColorStop(1, c2);
    g.fillStyle = grd; g.fillRect(0, 0, w, h);
    g.globalAlpha = .35;
    for (let i = 0; i < 7; i++) {
      g.beginPath();
      g.fillStyle = i % 2 ? '#ffffff' : c2;
      g.arc((i * 137) % w, (i * 89 + 40) % h, w * (0.08 + (i % 3) * 0.05), 0, Math.PI * 2);
      g.fill();
    }
    g.globalAlpha = 1;
    g.font = `${Math.round(h * 0.36)}px "Apple Color Emoji","Segoe UI Emoji","Noto Color Emoji",sans-serif`;
    g.textAlign = 'center'; g.textBaseline = 'middle';
    g.fillText(emoji, w / 2, h / 2);
    g.font = `600 ${Math.round(h * 0.05)}px "Be Vietnam Pro", sans-serif`;
    g.fillStyle = 'rgba(74,50,54,.5)';
    g.fillText('ảnh minh hoạ', w / 2, h - h * 0.08);
    return new Promise(r => cv.toBlob(r, 'image/jpeg', 0.85));
  }

  async function build(uid) {
    const journeys = [], checkins = [], photos = [];
    for (const T of TRIPS) {
      const stops = T.stops.map(p => ({ ...p, time: at(...p.t) }));
      let jid = null;
      if (T.route) {
        jid = uid();
        const points = buildRoute(stops, T.speed);
        let dist = 0;
        for (let i = 1; i < points.length; i++) dist += hav(points[i - 1], points[i]);
        const startedAt = stops[0].time - 20 * 60000;
        const endedAt = stops[stops.length - 1].time + 50 * 60000;
        const moving = points.length ? points[points.length - 1][2] - points[0][2] : 0;
        journeys.push({
          id: jid, name: T.name, note: T.note, startedAt, endedAt, points, distance: Math.round(dist),
          activeMs: Math.round(moving * 0.35), resumedAt: null, status: 'done', seg: 0, demo: true, ...T.region
        });
      }
      for (const p of stops) {
        const cid = uid();
        const photoIds = [];
        for (const [c1, c2, e] of p.p) {
          const pid = uid();
          const [full, thumb] = await Promise.all([drawPhoto(c1, c2, e, 960, 720), drawPhoto(c1, c2, e, 360, 270)]);
          photos.push({ id: pid, checkinId: cid, blob: full, thumb, createdAt: p.time, demo: true });
          photoIds.push(pid);
        }
        checkins.push({
          id: cid, journeyId: jid, lat: p.lat, lng: p.lng, name: p.name, address: '', cat: p.cat,
          rating: p.rating, price: p.price, mood: p.mood, tags: p.tags, feedback: p.feedback, memory: p.memory,
          photoIds, visitedAt: p.time, createdAt: p.time, updatedAt: p.time, demo: true, ...T.region, regionSrc: 'manual'
        });
      }
    }
    return { journeys, checkins, photos };
  }

  return { build };
})();
