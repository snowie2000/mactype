#include "png_encoder.h"

#include <Windows.h>
#include <Wincodec.h>

#include <limits>
#include <sstream>

namespace mactype {

std::vector<std::uint8_t> encode_png(std::uint32_t width, std::uint32_t height,
                                     std::uint32_t stride, const std::uint8_t* pixels,
                                     std::string& error) {
  error.clear();
  if (width == 0U || height == 0U || !pixels || width > std::numeric_limits<UINT>::max() / 4U ||
      stride < width * 4U || height > std::numeric_limits<UINT>::max() / stride) {
    error = "PNG dimensions, stride, or pixels are invalid";
    return {};
  }
  IWICImagingFactory* factory{};
  IWICBitmapEncoder* encoder{};
  IWICBitmapFrameEncode* frame{};
  IPropertyBag2* properties{};
  IStream* stream{};
  std::vector<std::uint8_t> result;
  HRESULT status = CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER,
                                    IID_PPV_ARGS(&factory));
  if (SUCCEEDED(status)) status = CreateStreamOnHGlobal(nullptr, TRUE, &stream);
  if (SUCCEEDED(status)) status = factory->CreateEncoder(GUID_ContainerFormatPng, nullptr, &encoder);
  if (SUCCEEDED(status)) status = encoder->Initialize(stream, WICBitmapEncoderNoCache);
  if (SUCCEEDED(status)) status = encoder->CreateNewFrame(&frame, &properties);
  if (SUCCEEDED(status)) status = frame->Initialize(properties);
  if (SUCCEEDED(status)) status = frame->SetSize(width, height);
  WICPixelFormatGUID format = GUID_WICPixelFormat32bppBGRA;
  if (SUCCEEDED(status)) status = frame->SetPixelFormat(&format);
  if (SUCCEEDED(status) && format != GUID_WICPixelFormat32bppBGRA) status = E_FAIL;
  if (SUCCEEDED(status)) {
    status = frame->WritePixels(height, stride, stride * height, const_cast<BYTE*>(pixels));
  }
  if (SUCCEEDED(status)) status = frame->Commit();
  if (SUCCEEDED(status)) status = encoder->Commit();
  if (SUCCEEDED(status)) {
    HGLOBAL memory{};
    status = GetHGlobalFromStream(stream, &memory);
    if (SUCCEEDED(status)) {
      const SIZE_T size = GlobalSize(memory);
      const void* data = GlobalLock(memory);
      if (data) {
        const auto* begin = static_cast<const std::uint8_t*>(data);
        result.assign(begin, begin + size);
        GlobalUnlock(memory);
      } else {
        status = E_FAIL;
      }
    }
  }
  if (properties) properties->Release();
  if (frame) frame->Release();
  if (encoder) encoder->Release();
  if (stream) stream->Release();
  if (factory) factory->Release();
  if (FAILED(status)) {
    std::ostringstream message;
    message << "WIC PNG encoding failed: 0x" << std::hex << static_cast<unsigned long>(status);
    error = message.str();
    result.clear();
  }
  return result;
}

}  // namespace mactype
