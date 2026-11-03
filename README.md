# SonVu Raw Pick: lọc ảnh sự kiện tự động

Ứng dụng cài trên máy (Windows 10/11 và macOS) cho dân media: thả thư mục ảnh RAW/JPEG của một sự kiện
vào, ứng dụng tự chấm điểm từng ảnh trên thang 10, loại ảnh hỏng và xuất ra thư mục đã lọc. Chạy hoàn
toàn bằng CPU, không cần card đồ hoạ, không gửi ảnh đi đâu.

## Dùng thế nào

1. Mở SonVu Raw Pick, kéo thư mục ảnh vào (hoặc bấm để chọn). Mỗi thư mục là một **phiên** có tab riêng;
   bấm **+ Phiên mới** hoặc kéo nhiều thư mục vào cùng lúc để lọc nhiều sự kiện. Các phiên được phân tích
   lần lượt để máy cũ không bị đuối, và tự mở lại khi bật ứng dụng (giữ cả các ảnh bạn sửa tay).
   Có thẻ nhớ thì bấm **Chép từ thẻ nhớ…** để chép an toàn trước (kiểm tra checksum, có thể sao lưu thêm một ổ).
2. Ứng dụng chấm điểm và chia ảnh thành **Đã chọn**, **Cần xem lại**, **Bị loại** kèm lý do.
3. Bấm vào ảnh để xem điểm từng tiêu chí, khung khuôn mặt, mắt nhắm. Cuộn chuột hoặc chụm hai ngón để phóng
   to, kéo để di chuyển, nhấp đúp (hoặc phím `1`) để xem 100% kiểm tra độ nét, phím `0` về vừa khung.
   Phím `P` chọn, `X` loại, `U` trả về quyết định của AI, `←` `→` chuyển ảnh.
4. Bấm **Trò chuyện với AI** để yêu cầu bằng lời: «ưu tiên ảnh có người cười», «chỉ giữ 300 ảnh»,
   «tôi thích ảnh tông tối», «chọn theo phong cách của tôi»… AI chỉnh cách chấm và chọn lại ngay.
   Gợi ý bấm nhanh chia 6 nhóm: loại sự kiện (đám cưới, hội nghị, sân khấu, tiệc, ảnh nhóm, thể thao),
   người & khuôn mặt (mắt mở, khẩu hình/há miệng, mặt nghiêng, nhân vật chính), bố cục & kỹ thuật,
   số lượng, gu của tôi, hỏi & hoàn tác («tóm tắt phiên này», «vì sao nhiều ảnh bị loại»,
   «AI đã học được gì từ tôi», «hoàn tác», «đặt lại mặc định»).
   AI trong máy hiểu các yêu cầu thường gặp, chạy offline. Máy nào đã cài **Claude Code** và đăng nhập tài
   khoản Claude thì bật «Dùng Claude» để hỏi tự do (dùng tài khoản của bạn, không cần API key).
5. Lấy ảnh ra theo một trong hai cách:
   - **Chép ảnh đã chọn vào thư mục con…**: chép ảnh đã chọn sang một thư mục con ngay trong thư mục gốc
     (ví dụ `Anh-da-chon/`). Ảnh gốc giữ nguyên tại chỗ; bản chép giống hệt từng byte, có kiểm tra checksum.
     Chép lại lần nữa thì bỏ qua file đã có; thư mục con này không bị phân tích lại.
   - **Xuất ảnh ra thư mục…**: chọn thư mục đích trên máy, ứng dụng tạo thư mục cùng tên thư mục ảnh
     thêm chữ `- Anh chon` (ví dụ `261103_khai giang tesol - Anh chon`) và để ảnh đã chọn ngay trong đó.
     Chọn **Chép** (giữ ảnh gốc) hoặc **Chuyển** (bản gốc chỉ bị xoá sau khi bản mới đã kiểm tra khớp).

```
Ten-su-kien - Anh chon/
  (ảnh đã chọn, giữ nguyên tên, định dạng và thư mục con của máy ảnh)
  Can-xem-lai/          (chỉ khi bật «Gồm cả ảnh Cần xem lại»)
  Bi-loai/              (chỉ khi bật «Gồm cả ảnh bị loại»)
    Nham-mat/  Mo-net/  Chay-sang/  Thieu-sang/  Nghieng/ ...
  bao-cao.csv, tong-ket.txt   (chỉ khi bật «Kèm file báo cáo»)
```

Mặc định chỉ xuất ảnh đã chọn, không kèm ảnh bị loại hay file báo cáo.

**Chất lượng ảnh giữ nguyên tuyệt đối**: file RAW, JPEG cùng tên và `.xmp` được chép hoặc chuyển nguyên
bản từng byte, không nén lại, không chuyển định dạng. Ảnh hiện trong ứng dụng chỉ là bản xem nhanh.

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

Mỗi lần bạn bấm **Chọn (P)** hoặc **Loại (X)** trên một ảnh, AI học ngay lựa chọn đó vào hồ sơ của
**loại sự kiện** đang chọn (Cưới, Hội nghị, Sân khấu…) và chấm lại cả thư mục. Đủ 20 lựa chọn (ít nhất
5 giữ, 5 bỏ) là AI bắt đầu chấm theo gu của bạn. Lựa chọn bằng tay được tính nặng gấp 3 ảnh để AI tự
quyết; càng nhiều lựa chọn và AI càng tái hiện đúng thì gu của bạn càng chiếm nhiều phần điểm (từ 50% khi đủ 20 lựa chọn, tới 95%),
và có thể giữ lại cả những ảnh mà luật cứng định loại (trừ mắt nhắm của nhân vật chính, ảnh hỏng,
ảnh trùng). Mỗi lần xuất, AI cũng học từ toàn bộ thư mục.

AI học từ: phơi sáng, độ nét, bố cục, vị trí và cỡ nhân vật chính, chủ thể sát mép, màu, nhiễu, mắt,
**khẩu hình miệng** (đang nói, há miệng), **góc mặt nghiêng**, số người, độ sáng tổng. Mặt nghiêng không
bị trừ điểm sẵn: AI theo gu của bạn. Ảnh đông người chỉ xét mắt của 2 người chính; mặt người đủ sáng thì
vùng cháy sáng ở nền (màn chiếu, đèn) chỉ bị trừ nhẹ. Mọi thứ nằm trên máy, không gửi đi đâu.

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

## Bản web (dùng song song, kể cả từ xa)

Ứng dụng trên máy tính tự mở thêm một bản web giống hệt giao diện trong app. Ảnh vẫn nằm và được xử lý
trên máy tính đó; trình duyệt chỉ nhận ảnh thu nhỏ và ảnh xem trước. Bản web có đủ quyền như app: duyệt
thư mục trên máy, lọc, gom, chuyển, xuất ảnh, chat với AI. Máy phải bật và đang mở SonVu Raw Pick.

Cài đặt nằm ở mục **Bản web** bên trái app:

- **Trên chính máy đó**: bấm link `http://127.0.0.1:8686` (mở sẵn, đã đăng nhập).
- **Điện thoại, máy khác cùng wifi**: bật «Cho điện thoại, máy khác cùng wifi vào», mở link
  `http://192.168.x.x:8686` hiện ra. Windows có thể hỏi cho phép qua tường lửa: chọn **Allow**.
- **Từ bất kỳ đâu qua internet**: bật «Vào từ bất kỳ đâu qua internet». Lần đầu app tải công cụ kết nối
  `cloudflared` của Cloudflare (miễn phí, không cần tài khoản) và hiện link dạng
  `https://xxx.trycloudflare.com`. Link này đổi mỗi lần mở app.
- **Địa chỉ cố định với tên miền riêng** (ví dụ `pick.tenmien.vn`), không cần thuê máy chủ:
  1. Tạo tài khoản Cloudflare (gói Free), thêm tên miền, đổi nameserver ở nơi mua tên miền theo hướng dẫn.
  2. Vào **Zero Trust → Networks → Tunnels → Create a tunnel** (loại Cloudflared), đặt tên, chép mã
     trong lệnh cài (đoạn dài sau `--token`, hoặc dán cả lệnh cũng được).
  3. Ở bước **Public Hostname**: Subdomain `pick`, Domain là tên miền của bạn, Service `HTTP`,
     URL `localhost:8686`.
  4. Trong app, mở «Địa chỉ cố định», điền `pick.tenmien.vn`, dán mã tunnel, bấm **Lưu**, rồi bật
     «Vào từ bất kỳ đâu qua internet».

Mọi trình duyệt đều phải nhập **mật khẩu** hiện trong app (trừ khi mở bằng link trong app). Nhập sai 5 lần
phải đợi 1 phút; bấm **Đổi** để tạo mật khẩu mới và đăng xuất mọi nơi. Ai có mật khẩu là điều khiển
được máy này, nên chỉ đưa cho người tin cậy.

## Cài đặt

Bộ cài được build tự động bằng GitHub Actions (tab **Actions** → **Đóng gói Windows và Mac** → tải
`SonVu-Raw-Pick-windows` hoặc `SonVu-Raw-Pick-macos`). Gắn tag `v0.1.0` thì bộ cài lên mục **Releases**.

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
