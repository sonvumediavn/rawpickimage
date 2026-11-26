# Mô hình AI

`face_detection_yunet_2023mar.onnx` (YuNet, giấy phép MIT, từ opencv_zoo) được tải tự động khi build
bằng `scripts/fetch-models.sh`, không lưu trong git. Ứng dụng vẫn chạy nếu thiếu file này, chỉ bỏ qua
tiêu chí «Người» (khuôn mặt, mắt nhắm).

`face_recognition_sface_2021dec.onnx` (SFace, Apache-2.0, opencv_zoo, 39 MB) cũng được tải tự động:
nhận ra từng người để tách các nhóm, các người lên sân khấu lần lượt. Thiếu file này thì app vẫn gom
ảnh theo hình và số người như trước.

`facial_expression_recognition_mobilefacenet_2022july.onnx` (MobileFaceNet FER, Apache-2.0, opencv_zoo,
5 MB) đọc nét mặt (cười, bình thường, ngạc nhiên, khó chịu, buồn…) của các khuôn mặt đủ lớn, dùng chung
cách căn mặt với SFace. AI học gu dùng thêm ba con số: mọi người cười nhiều hay ít, người có nét mặt khó
chịu nhất, nhân vật chính có cười không. Thiếu file thì app bỏ qua phần này.

`pose_estimation_mediapipe_2023mar.onnx` (MediaPipe Pose, Apache-2.0, opencv_zoo, 5,5 MB) đọc dáng người
của tối đa 3 người chính trong ảnh: khung cắt ngang khớp (đầu gối, cổ chân, cổ tay, khuỷu tay), tay che
mặt, tay đang diễn tả, khoanh tay, xoay người khỏi máy, hai người trao tay nhau. Tải tự động.

`nima_mobilenet_aesthetic.onnx` (NIMA MobileNet của idealo/image-quality-assessment, Apache-2.0, 13 MB) chấm
"đẹp tổng thể" của cả bức ảnh như số đông người xem (huấn luyện trên 250 000 ảnh AVA). File này được
chuyển từ Keras sang ONNX một lần và lưu thẳng trong git (không có bản ONNX công khai để tải).
