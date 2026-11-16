# Dấu Chân Hồng

Web app chạy trên điện thoại để ghi lại hành trình trên bản đồ thật, check-in từng nơi đã đến, lưu kỉ niệm, feedback và ảnh. Tông màu hồng, trắng và be.

## Tính năng

- **Bản đồ thật** (OpenStreetMap / CARTO, có thêm chế độ vệ tinh). Có ô tìm địa điểm và nút về vị trí của bạn.
- **Ghi hành trình bằng GPS**: bắt đầu, tạm dừng, tiếp tục, kết thúc. Lộ trình được vẽ trực tiếp trên bản đồ, kèm quãng đường, thời gian, tốc độ trung bình và số bước chân ước tính. App tự lọc các điểm GPS nhiễu.
- **Check-in**: lấy vị trí hiện tại, bấm giữ trên bản đồ, hoặc chọn từ kết quả tìm kiếm. Tên đường và tên quán được tự điền.
  - Loại địa điểm (quán ăn, cà phê, tham quan…), chấm tim 1–5, mức giá, thẻ nhận xét nhanh
  - Feedback về địa điểm, kỉ niệm, tâm trạng, thời gian
  - **Tải ảnh lên** (tối đa 12 ảnh mỗi điểm, tự nén cho nhẹ)
- **Kỉ niệm**: dòng thời gian theo ngày, album ảnh, tìm kiếm và lọc theo loại / yêu thích / có ảnh. Xem ảnh toàn màn hình, có nút chỉ đường bằng Google Maps.
- **Hành trình**: danh sách kèm hình thu nhỏ của lộ trình. Xem lại lộ trình trên bản đồ, đổi tên, ghi cảm nhận.
- **Thống kê**: tổng km, số bước chân, số hành trình, check-in, địa điểm, ảnh, ngày khám phá, giờ di chuyển; biểu đồ theo loại địa điểm, check-in theo tháng, phân bố đánh giá, top nơi yêu thích, hành trình dài nhất, tâm trạng.
- **Dữ liệu ở ngay trên máy** (IndexedDB), không cần tài khoản. Có sao lưu/khôi phục ra tệp `.json` (kèm ảnh) và dữ liệu mẫu để xem thử.
- **PWA**: cài lên màn hình chính như app, mở được khi không có mạng (các ô bản đồ đã xem được lưu tạm).

## Chạy thử trên máy

```bash
python3 -m http.server 8080
# mở http://localhost:8080
```

## Dùng trên điện thoại

Trình duyệt chỉ cho truy cập GPS khi trang chạy qua **HTTPS**. Cách nhanh nhất là bật **GitHub Pages** cho repo này
(Settings → Pages → Deploy from a branch → chọn nhánh và thư mục `/`), rồi mở link đó trên điện thoại:

- iPhone (Safari): nút Chia sẻ → *Thêm vào MH chính*
- Android (Chrome): menu ⋮ → *Cài đặt ứng dụng* / *Thêm vào màn hình chính*

Bạn cũng có thể kéo cả thư mục vào Netlify Drop hoặc deploy lên Vercel, Cloudflare Pages… vì đây là trang tĩnh, không cần build.

> Lưu ý: web app chỉ ghi được GPS khi màn hình đang mở và app đang hiển thị. App tự giữ màn hình sáng khi đang ghi (nếu trình duyệt hỗ trợ).

## Cấu trúc

```
index.html            khung giao diện
css/style.css         giao diện (token màu hồng, trắng, be)
js/app.js             bản đồ, GPS, check-in, kỉ niệm, thống kê
js/db.js              lưu trữ IndexedDB
js/demo.js            dữ liệu mẫu (Đà Lạt & Hà Nội)
js/icons.js           bộ icon SVG
sw.js                 service worker (offline)
vendor/leaflet/       thư viện bản đồ Leaflet 1.9.4
```

Dịch vụ dùng: ô bản đồ © OpenStreetMap contributors, © CARTO, ảnh vệ tinh © Esri; tìm kiếm và lấy tên địa chỉ qua Nominatim (OpenStreetMap).
