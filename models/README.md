# Mô hình AI

`face_detection_yunet_2023mar.onnx` (YuNet, giấy phép MIT, từ opencv_zoo) được tải tự động khi build
bằng `scripts/fetch-models.sh`, không lưu trong git. Ứng dụng vẫn chạy nếu thiếu file này, chỉ bỏ qua
tiêu chí «Người» (khuôn mặt, mắt nhắm).

`face_recognition_sface_2021dec.onnx` (SFace, Apache-2.0, opencv_zoo, 39 MB) cũng được tải tự động:
nhận ra từng người để tách các nhóm, các người lên sân khấu lần lượt. Thiếu file này thì app vẫn gom
ảnh theo hình và số người như trước.
