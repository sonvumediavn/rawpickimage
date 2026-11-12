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
5. Bấm **Xuất ảnh chọn** (một chạm):
   - Ảnh đã chọn được chép vào thư mục con mang tên thư mục gốc thêm `_anh chon`, ngay trong thư mục gốc,
     để lưu trữ cùng show. Ví dụ `261104_van nghe cong doan/261104_van nghe cong doan_anh chon/`.
     Ảnh gốc giữ nguyên; bản chép giống hệt từng byte, có kiểm tra checksum. Xuất lại thì bỏ qua file đã có.
   - Sau đó cả thư mục được chép tiếp vào thư mục **Google Drive** bạn chọn (app Google Drive for desktop
     trên máy tự tải lên). Lần đầu app hỏi thư mục Drive, từ đó nhớ luôn, đổi ở mục «Khi xuất».
     Mẹo: đặt thư mục Drive đó ở chế độ «Bất kỳ ai có đường liên kết đều xem được» một lần, mọi show
     bên trong tự có quyền xem. Trên Mac app tự chép link thư mục; trên Windows chuột phải thư mục →
     Google Drive → «Sao chép đường liên kết».
   - **Xuất ra thư mục khác…** vẫn có khi cần chép (hoặc chuyển) sang một chỗ khác.

   Công tắc **AI tự chọn** trên thanh công cụ: tắt thì AI vẫn chấm điểm, ghi chú và gợi ý từng ảnh nhưng
   không chọn sẵn ảnh nào (tab «Chưa chọn»), bạn tự chọn bằng P / X. Bật lại thì AI chọn như cũ; ảnh bạn
   đã chọn tay giữ nguyên ở cả hai chế độ.

```
Ten-su-kien_anh chon/
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

**AI nhìn ảnh**: ngoài các con số đo được, mỗi ảnh có một «dấu vân hình» do mô hình MobileNetV2 (chạy
trên CPU, không cần mạng) tạo ra. AI so ảnh mới với những ảnh bạn từng giữ và từng bỏ ở các show khác,
nên học được cả kiểu khoảnh khắc, khung hình, dáng người bạn thích. Dấu vân hình gồm ba phần: trong
ảnh có gì, cái gì nằm ở góc nào của khung (bốn phần tư khung hình), và chỗ mắt người xem bị hút vào
(chủ thể đặt ở đâu, to hay nhỏ). Nhờ đó AI nhớ được bố cục và góc máy bạn hay chọn, không cần Claude.

**Mắt mở hay nhắm**: một mô hình nhỏ chuyên nhận mắt (open-closed-eye-0001 của OpenVINO, 46 KB, chạy
trên CPU) xem từng mắt, kết hợp với phép đo độ cao tròng đen. Nếu AI nhìn sai, mở ảnh và bấm «Mở» hoặc
«Nhắm» cạnh từng người: ảnh đó sửa ngay, và AI chỉnh lại cách xét mắt cho mọi ảnh khác theo những lần
bạn sửa. Khi bạn chọn một ảnh mà AI tưởng là nhắm mắt, AI cũng coi đó là gợi ý nhẹ rằng mắt đang mở.
Các lần sửa mắt cũng được chia sẻ qua thư mục học chung giữa các máy.

**Khẩu hình, góc mặt, hướng nhìn**: cạnh từng người còn có nút bật tắt giống nút mắt: Khẩu hình
(Đẹp / Xấu), Góc mặt (Thẳng / Nghiêng), Hướng nhìn (Ngẩng / Cúi). AI tự đoán sẵn từ các điểm mốc khuôn
mặt (miệng há bao nhiêu, mặt quay bao nhiêu, mũi nằm thấp hay cao so với mắt và miệng). Bấm sửa khi AI
đoán sai: ảnh đó đổi ngay, và AI học lại ngưỡng của bạn cho mọi ảnh khác (ghi «bạn sửa», bấm ↺ để trả
về AI). Người phát biểu một mình trong khung mà cúi đầu (đọc giấy) thì vào «Cần xem lại»; nói với AI
«người phát biểu cúi đầu vẫn giữ» để tắt.

**Ảnh gắn sao trên máy ảnh**: app đọc số sao bạn gắn ngay trên máy (Canon, Nikon, Sony, Fujifilm,
cả sao gắn sau trong Lightroom/Bridge qua file .xmp) và ảnh khóa bảo vệ. Khi thư mục có ảnh gắn sao,
thanh công cụ hiện ô ★: «Chỉ lọc ảnh có sao» (bỏ ảnh không sao, AI lọc tiếp trong số ảnh có sao: mắt
nhắm, mờ, trùng…) hoặc «Lấy luôn ảnh có sao» (chọn đúng những ảnh bạn gắn sao).

Tab **★ Gắn sao** gom riêng ảnh có sao, lọc nhanh theo 1★ đến 5★ (ví dụ «3★ trở lên»). Trong tab có:
- **Dạy AI theo ảnh gắn sao**: ảnh có sao (theo mức đang lọc) là ảnh bạn giữ, còn lại là ảnh bỏ, AI học
  cả thư mục như khi bạn chọn tay.
- **Xuất ảnh gắn sao**: chép đúng những ảnh đó (kèm RAW/JPEG/.xmp cùng tên) vào thư mục `_anh chon`.

**★ Chép nhanh ảnh gắn sao…** (thanh công cụ, cần gấp khi chưa kịp lọc trên máy): chọn thẳng thẻ nhớ
(hoặc thư mục bất kỳ) và nơi lưu, chọn mức sao. App chỉ đọc số sao, không phân tích ảnh, rồi chép những
ảnh đó vào `<nơi lưu>/<tên thư mục nguồn>_anh sao/`, có kiểm tra checksum, ảnh trên thẻ giữ nguyên.
Tích «Chép xong thì cho AI lọc tiếp các ảnh này» nếu muốn AI chấm tiếp.

**Ảnh chụp liên tiếp**: các tấm trong một loạt được gom theo hình (cùng cảnh, cùng người, xê dịch hay zoom
nhẹ vẫn là một loạt), mỗi loạt chỉ giữ tấm đẹp nhất (hoặc số tấm bạn đặt ở «Mỗi loạt giữ»). App so cả vị
trí và số người trong khung: 6 nhóm lần lượt lên nhận giải trên cùng sân khấu là 6 cảnh khác nhau, không
gộp làm một. App còn **nhận ra từng người** (mô hình SFace, chạy trên CPU, cho tối đa 6 khuôn mặt lớn
nhất mỗi ảnh): sinh viên lần lượt lên nhận bằng từ cùng một lãnh đạo, đứng đúng một chỗ, chụp cách nhau
vài giây, vẫn là mỗi người một cảnh, mỗi cảnh giữ tấm đẹp nhất. Ảnh đông người ưu tiên tấm có nhiều
người mở mắt và ngẩng đầu nhất.
Cảnh nào bạn bấm từ 3 tấm trở lên (cảnh có từ 2 người: từ 2 tấm) mà AI chưa chọn tấm nào, AI giữ tấm
đẹp nhất nếu dùng được (không
nhắm mắt, không mờ mặt), để mỗi nhóm có một ảnh. Nói với AI «mỗi nhóm nhận giải giữ 1 ảnh» để bật lại
nếu đã tắt.

**Crop ảnh** (phím C trong trình xem ảnh): kéo khung giữ lại, chọn tỉ lệ (tự do, như gốc, 1:1, 3:2, 4:3,
4:5, 16:9, 9:16…), Enter lưu. Giữ **Shift** khi kéo thì khung giữ nguyên tỉ lệ, chỉ to nhỏ; cuộn chuột
cũng phóng to thu nhỏ khung. Lưu xong app hiện ảnh thành quả (bấm «Sửa crop» để chỉnh tiếp).
**Ctrl+Z** hoàn tác, **Ctrl+Y** (hoặc Ctrl+Shift+Z, trên Mac dùng ⌘) làm lại, cả khi đang kéo khung lẫn
với crop đã lưu. Ảnh gốc không bao giờ bị sửa; chỉ bản xuất được cắt:
- JPEG: cắt thẳng trên dữ liệu nén, phần giữ lại giống từng điểm ảnh của file gốc, không nén lại, giữ EXIF
  và màu. Mép trái/trên có thể rộng hơn khung vài điểm ảnh (khối 8–16 px của JPEG), không bao giờ hẹp hơn.
- RAW: file RAW chép nguyên vẹn, kèm khung crop trong file `.xmp` (Lightroom, Camera Raw, Bridge mở ra
  là đã crop, vẫn chỉnh lại được) và một JPEG đã cắt từ ảnh JPEG cỡ lớn máy ảnh nhúng trong RAW. Chọn
  kiểu ở mục «Khi xuất». Có file JPEG đi kèm RAW thì JPEG đó được cắt, không tạo thêm.

**Học từ show cũ…**: chọn thư mục một show đã lọc xong, xếp kiểu gì cũng được. App tự nhận ra thư mục
ảnh bạn đã chọn bằng hình (cùng giờ chụp và trông giống nhau, kể cả đã đổi tên, chỉnh màu, xuất lại),
coi ảnh gốc có bản trong đó là ảnh bạn giữ, còn lại là ảnh bỏ, rồi học luôn cả show. Chỉ có ảnh đã chọn
thì app coi tất cả là ảnh bạn thích.

**Làm trên nhiều máy (Win và Mac)**: ở mục «AI học theo bạn», chọn cùng một thư mục trên Google Drive ở
mỗi máy. Mỗi máy ghi lựa chọn của mình vào file riêng trong đó (không bao giờ ghi đè nhau); khi mở hay
chấm ảnh, app gộp lựa chọn của mọi máy rồi học lại. Cài lại hay cập nhật app không mất gì: dữ liệu học
nằm ở `%LOCALAPPDATA%\vn.sonvumedia.rawpick` (Windows) hoặc `~/Library/Application Support/vn.sonvumedia.rawpick` (Mac).

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
