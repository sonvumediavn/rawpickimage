# Dấu Chân Hồng

Web app chạy trên điện thoại: bản đồ chữ S của riêng bạn. Mỗi nơi đã đến là một chiếc ghim, gom theo quốc gia, tỉnh thành, và phóng to để thấy từng quán. Giao diện kính mờ (glass), tông hồng, trắng và be.

## App được chia thế nào

| Tab | Trả lời câu hỏi | Có gì |
| --- | --- | --- |
| **Bản đồ** | Mình đã đến những đâu? | Bản đồ thật, mặc định vừa khung Việt Nam. Ghim tự gom theo cấp: thế giới → quốc gia → tỉnh/thành → từng địa điểm. |
| **Nhật ký** | Hôm đó mình đã làm gì? | Mọi thứ theo từng ngày: lộ trình đã ghi, các điểm check-in theo giờ, ảnh và kỉ niệm. Có chế độ Album. |
| **Địa điểm** | Quán nào đáng quay lại? | Sổ tay quán ăn, chỗ chơi kèm feedback, xếp theo tỉnh/thành. Lọc theo loại, yêu thích, “đáng quay lại”. |
| **Dấu chân** | Mình đã đi được bao nhiêu? | Hộ chiếu 34 tỉnh thành, sổ đóng dấu theo miền, số km, biểu đồ, top nơi yêu thích. Sao lưu dữ liệu. |

Nút **+** ở giữa gom mọi thao tác tạo mới: check-in tại chỗ, ghim một điểm trên bản đồ, bắt đầu/tạm dừng/kết thúc ghi hành trình.

## Bản đồ theo cấp zoom

- **Thu nhỏ hết cỡ**: mỗi quốc gia một bong bóng (cờ + số địa điểm).
- **Bản đồ chữ S** (mặc định): mỗi tỉnh/thành đã đến một bong bóng. Thẻ ở đáy cho biết đã đi mấy trên 34 tỉnh thành.
- **Phóng to vào một tỉnh**: ghim từng địa điểm, các điểm sát nhau gom thành cụm; lộ trình đã ghi hiện dạng nét đứt.
- **Phóng to nữa**: ghim hiện ảnh và tên địa điểm.

Thẻ ở đáy bản đồ luôn đi theo vùng đang xem (Thế giới › Việt Nam › Lâm Đồng); chạm vào từng cấp để nhảy tới đó.

Tỉnh/thành theo đơn vị hành chính sau sáp nhập 1/7/2025 (34 tỉnh thành). Tên tỉnh cũ được tự quy về đơn vị mới (ví dụ Hội An → Đà Nẵng, Mũi Né → Lâm Đồng). App nhận tỉnh từ dịch vụ bản đồ khi có mạng, và tự đoán theo vị trí khi không có mạng; bạn có thể sửa tay trong phần “Thêm chi tiết” khi check-in.

## Check-in

- Vị trí lấy từ GPS, bấm giữ trên bản đồ, kết quả tìm kiếm, hoặc tự đặt ghim.
- Tên quán, địa chỉ, loại địa điểm và tỉnh/thành được điền sẵn khi có thể. Nơi đã từng ghé được gợi ý lại.
- Ảnh (tối đa 12, tự nén), chấm tim 1–5, feedback, kỉ niệm.
- Thêm chi tiết: mức giá, thẻ nhận xét nhanh, tâm trạng, thời gian.
- Ghé lại một nơi nhiều lần được gộp thành một địa điểm với nhiều lần ghé.

## Ghi hành trình

Bắt đầu, tạm dừng, tiếp tục, kết thúc. Lộ trình vẽ trực tiếp trên bản đồ, kèm quãng đường, thời gian, tốc độ trung bình và số bước chân ước tính. App tự bỏ qua điểm GPS nhiễu.

> Web app chỉ ghi được GPS khi màn hình đang mở và app đang hiển thị. App tự giữ màn hình sáng khi đang ghi nếu trình duyệt hỗ trợ.

## Dữ liệu

Mọi thứ nằm ngay trên máy (IndexedDB), không cần tài khoản. Có sao lưu/khôi phục ra tệp `.json` kèm ảnh, và bộ dữ liệu mẫu để xem thử.

## Chạy thử trên máy

```bash
python3 -m http.server 8080
# mở http://localhost:8080
```

## Dùng trên điện thoại

Trình duyệt chỉ cho truy cập GPS khi trang chạy qua **HTTPS**. Cách nhanh nhất là bật **GitHub Pages** cho repo này
(Settings → Pages → Deploy from a branch → chọn nhánh và thư mục `/`), rồi mở link đó trên điện thoại:

- iPhone (Safari): nút Chia sẻ → *Thêm vào MH chính*
- Android (Chrome): menu ⋮ → *Cài đặt ứng dụng*

Đây là trang tĩnh, không cần build, nên cũng có thể đưa lên Netlify, Vercel, Cloudflare Pages.

## Cấu trúc

```
index.html            khung giao diện
css/style.css         giao diện glass (token màu hồng, trắng, be)
js/app.js             bản đồ, GPS, check-in, nhật ký, địa điểm, dấu chân
js/geo.js             34 tỉnh thành, quy đổi tên cũ, nhận diện tỉnh/quốc gia
js/vn-shape.js        đường viền Việt Nam (Natural Earth, dữ liệu công cộng)
js/db.js              lưu trữ IndexedDB
js/demo.js            dữ liệu mẫu
js/icons.js           bộ icon SVG
sw.js                 service worker (offline)
vendor/leaflet/       thư viện bản đồ Leaflet 1.9.4
```

Dịch vụ dùng: ô bản đồ © OpenStreetMap contributors, © CARTO, ảnh vệ tinh © Esri; tìm kiếm và lấy địa chỉ qua Nominatim (OpenStreetMap).
