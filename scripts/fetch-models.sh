#!/usr/bin/env bash
# Tải mô hình nhận diện khuôn mặt YuNet (MIT, OpenCV Zoo) vào models/.
# Chạy tự động trong GitHub Actions; trên máy dev chạy tay một lần.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p models
f=models/face_detection_yunet_2023mar.onnx
url=https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx
if [ ! -s "$f" ]; then
  curl -fL --retry 4 --retry-delay 3 -o "$f.tmp" "$url"
  mv "$f.tmp" "$f"
fi
size=$(wc -c < "$f")
if [ "$size" -lt 100000 ]; then
  echo "File mô hình quá nhỏ ($size byte), có thể tải nhầm trang LFS" >&2
  exit 1
fi
if command -v sha256sum >/dev/null; then sum=$(sha256sum "$f" | cut -d' ' -f1); else sum=$(shasum -a 256 "$f" | cut -d' ' -f1); fi
echo "YuNet: $size byte, sha256 $sum"
if [ -n "${YUNET_SHA256:-}" ] && [ "$sum" != "$YUNET_SHA256" ]; then
  echo "sha256 không khớp (mong đợi $YUNET_SHA256)" >&2
  exit 1
fi

# Mô hình nhìn ảnh MobileNetV2 (Apache-2.0, ONNX Model Zoo): AI học gu chọn ảnh từ chính hình ảnh.
f=models/mobilenetv2-12.onnx
url=https://media.githubusercontent.com/media/onnx/models/main/validated/vision/classification/mobilenet/model/mobilenetv2-12.onnx
want=c0c3f76d93fa3fd6580652a45618618a220fced18babf65774ed169de0432ad5
if [ ! -s "$f" ]; then
  curl -fL --retry 4 --retry-delay 3 -o "$f.tmp" "$url"
  mv "$f.tmp" "$f"
fi
if command -v sha256sum >/dev/null; then sum=$(sha256sum "$f" | cut -d' ' -f1); else sum=$(shasum -a 256 "$f" | cut -d' ' -f1); fi
echo "MobileNetV2: $(wc -c < "$f") byte, sha256 $sum"
if [ "$sum" != "$want" ]; then
  echo "sha256 của MobileNetV2 không khớp (mong đợi $want)" >&2
  exit 1
fi

# Mô hình nhận mắt mở/nhắm open-closed-eye-0001 (Apache-2.0, OpenVINO Open Model Zoo), 46 KB.
f=models/open_closed_eye.onnx
url=https://storage.openvinotoolkit.org/repositories/open_model_zoo/public/2022.1/open-closed-eye-0001/open_closed_eye.onnx
want=4daa100034482525a26c9afb9297c16580a531189e66e3d2b2ac7d32becfd593
if [ ! -s "$f" ]; then
  curl -fL --retry 4 --retry-delay 3 -o "$f.tmp" "$url"
  mv "$f.tmp" "$f"
fi
if command -v sha256sum >/dev/null; then sum=$(sha256sum "$f" | cut -d' ' -f1); else sum=$(shasum -a 256 "$f" | cut -d' ' -f1); fi
echo "Mắt mở/nhắm: $(wc -c < "$f") byte, sha256 $sum"
if [ "$sum" != "$want" ]; then
  echo "sha256 của mô hình mắt không khớp (mong đợi $want)" >&2
  exit 1
fi
