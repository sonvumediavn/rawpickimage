# RawPick: lọc ảnh sự kiện tự động

Ứng dụng cài trên máy (Windows 10/11 và macOS) cho dân media: thả thư mục ảnh RAW/JPEG của một sự kiện
vào, ứng dụng tự chấm điểm từng ảnh trên thang 10, loại ảnh hỏng và xuất ra thư mục đã lọc. Chạy hoàn
toàn bằng CPU, không cần card đồ hoạ, không gửi ảnh đi đâu.

## Dùng thế nào

1. Mở RawPick, kéo thư mục ảnh vào ô bên trái (hoặc bấm để chọn). Có thẻ nhớ thì bấm **Chép từ thẻ nhớ…**
   để chép an toàn trước (kiểm tra checksum, có thể sao lưu thêm một ổ).
2. Ứng dụng phân tích và chia ảnh thành **Đã chọn**, **Cần xem lại**, **Bị loại** kèm lý do.
3. Bấm vào ảnh để xem điểm từng tiêu chí, khung khuôn mặt, mắt nhắm. Phím `P` chọn, `X` loại,
   `U` trả về quyết định của AI, `←` `→` chuyển ảnh.
4. Bấm **Xuất thư mục đã lọc**. Kết quả nằm cạnh thư mục gốc:

```
Ten-su-kien_da-loc/
  1_Da-chon/
  2_Can-xem-lai/
  3_Bi-loai/
    Nham-mat/  Mo-net/  Chay-sang/  Thieu-sang/  Nghieng/
    Bo-cuc/    Nhieu/   Trung-lap/  Mat-loi/     Diem-thap/
  bao-cao.csv      (mở bằng Excel: điểm, lý do, gợi ý sửa từng ảnh)
  tong-ket.txt     (thống kê buổi chụp, theo từng máy)
```

Ảnh gốc **không bao giờ bị di chuyển hay xoá**. File RAW, JPEG cùng tên và file `.xmp` đi chung với nhau.

## Ứng dụng chấm những gì

| Tiêu chí | Đo bằng |
|---|---|
| Phơi sáng | Histogram: cháy sáng, đen kịt, quá sáng/tối, tương phản |
| Độ nét | Laplacian theo ô, lấy vùng nét nhất (chủ thể), trừ nhiễu |
| Bố cục | Bản đồ chủ thể, quy tắc 1/3, chủ thể sát mép, đường chân trời nghiêng |
| Màu sắc | Độ rực màu, ám màu |
| Nhiễu | Ước lượng nhiễu theo ISO |
| Người | YuNet tìm khuôn mặt, đo mắt nhắm, mặt mờ, mặt tối, quay nghiêng, bị cắt khung |

Điểm cuối là trung bình có trọng số (chọn kiểu Sự kiện, Cân bằng, Chân dung, Phong cảnh hoặc tự kéo),
có trần cho lỗi nặng: ảnh mờ tối đa 4,5; phơi sáng hỏng tối đa 5; người nhắm mắt tối đa 5,5.

## Vì sao chạy nhanh trên máy cũ

- Không giải mã RAW: đọc ảnh JPEG xem trước mà máy ảnh đã nhúng sẵn trong file RAW (chỉ đọc đúng
  đoạn byte đó), giải mã ở 1/2 đến 1/8 kích thước ngay trong bộ giải mã JPEG.
- Hai lượt: ảnh hỏng nặng (mờ, phơi sáng hỏng) bị loại ngay, không chạy AI khuôn mặt.
- AI khuôn mặt YuNet chỉ 230 KB, chạy bằng tract (Rust thuần), không cần GPU hay thư viện ngoài.
- Dùng mọi nhân CPU trừ một nhân để giao diện vẫn mượt. Mở lại thư mục cũ thì lấy kết quả đã lưu.

Chạy thử trên máy dev: khoảng 60 ms/ảnh mỗi luồng với ảnh JPEG 1024 px (chưa tính AI khuôn mặt).
Cần đo lại trên máy Windows cũ của Sơn với thư mục RAW thật.

## AI học theo cách bạn chọn

Mỗi lần xuất thư mục, ứng dụng ghi lại ảnh nào bạn giữ, ảnh nào bỏ (kể cả những ảnh bạn sửa tay) vào
hồ sơ của **loại sự kiện** đang chọn (Cưới, Hội nghị, Sân khấu…). Đủ 40 ảnh có cả giữ và bỏ, AI bắt
đầu điều chỉnh điểm theo gu của bạn và cho biết nó đã học được gì, đồng ý với bạn bao nhiêu phần trăm.
Mọi thứ nằm trên máy, không gửi đi đâu.

## Các tuỳ chọn

AI trong ứng dụng gợi ý nên bật gì cho từng thư mục, bạn quyết định. Nếu bật «Để AI tự bật», AI chỉ được
tự bật những việc chạy trên máy; việc chia sẻ, đăng bài luôn chờ bạn.

| Tuỳ chọn | Trạng thái |
|---|---|
| Đủ khoảnh khắc: mỗi khoảnh khắc giữ ít nhất 1 ảnh | Có |
| Sửa thay vì loại: ảnh hơi nghiêng, lệch sáng nhẹ vào «Cần xem lại» kèm gợi ý sửa | Có |
| Gộp nhiều máy: chung dòng thời gian, chỉnh lệch giờ từng máy | Có |
| Chép thẻ an toàn | Có |
| Báo cáo buổi chụp | Có (cảnh báo bụi cảm biến: sau) |
| Nhân vật chính (cô dâu, chú rể, diễn giả) | Giai đoạn 2 |
| Giao ảnh ngay lên Google Drive | Giai đoạn 2 |
| Logo / watermark | Giai đoạn 2 |
| Kiểm tra trước khi đăng fanpage | Giai đoạn 3 |
| Khách tìm ảnh bằng selfie | Giai đoạn 4 |

## Cài đặt

Bộ cài được build tự động bằng GitHub Actions (tab **Actions** → **Đóng gói Windows và Mac** → tải
`RawPick-windows` hoặc `RawPick-macos`). Gắn tag `v0.1.0` thì bộ cài lên mục **Releases**.

- Windows 10/11: chạy file `.exe`. Bộ cài chưa ký số nên Windows có thể hiện «Windows protected your PC»:
  bấm **More info** → **Run anyway**. Bộ cài tự cài WebView2 nếu máy chưa có.
- macOS 10.15 trở lên (Intel và Apple Silicon): mở `.dmg`, kéo vào Applications. Lần đầu bấm chuột phải
  → **Open** vì ứng dụng chưa ký với Apple.

## Dành cho người phát triển

```
scripts/fetch-models.sh               # tải mô hình YuNet vào models/
cargo test -p rawpick-core            # kiểm thử lõi
cargo run --release -p rawpick-core --bin rawpick -- cull "/đường/dẫn/thư-mục" --model models/face_detection_yunet_2023mar.onnx --dry-run
cd app && npx @tauri-apps/cli@2 dev   # chạy ứng dụng (Linux cần libwebkit2gtk-4.1-dev)
```

- `core/`: thư viện Rust: đọc RAW/EXIF (`exif.rs`, `preview.rs`), đo chất lượng (`metrics.rs`),
  khuôn mặt (`faces.rs`), quyết định (`cull.rs`), học (`learn.rs`), tuỳ chọn và gợi ý (`options.rs`),
  xuất (`export.rs`), chép thẻ (`ingest.rs`), quy trình (`pipeline.rs`), và công cụ dòng lệnh `rawpick`.
- `app/`: ứng dụng Tauri 2 (`src-tauri/`) và giao diện HTML/JS thuần (`ui/`).
