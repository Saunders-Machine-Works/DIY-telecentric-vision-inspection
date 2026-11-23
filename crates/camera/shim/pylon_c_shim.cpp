// macOS stand-in for the subset of the Basler pylon C API (PylonC) that
// crates/camera/src/pylon.rs uses. pylon for macOS ships only the C++ API, so
// each function here maps onto the C++ low-level API (IPylonDevice,
// IStreamGrabber, WaitObject, GenApi node maps) and keeps PylonC's contract:
// return 0 on success, non-zero on failure with the message available from
// GenApiGetLastErrorMessage. No C++ exception crosses into Rust.
//
// The structs below mirror the #[repr(C)] structs in pylon.rs; the
// static_asserts keep the two in step.

#include <pylon/PylonIncludes.h>

#include <cstdint>
#include <cstring>
#include <mutex>
#include <string>

using namespace Pylon;

namespace {

constexpr size_t kFullNameLen = 260;  // pylon.rs FILENAME_MAX
constexpr size_t kInfoStrLen = 64;    // pylon.rs DEVICE_INFO_STR_LEN

struct ShimDeviceInfo {
    char full_name[kFullNameLen];
    char friendly_name[kInfoStrLen];
    char vendor_name[kInfoStrLen];
    char model_name[kInfoStrLen];
    char serial_number[kInfoStrLen];
    char device_class[kInfoStrLen];
    char device_version[kInfoStrLen];
    char user_defined_name[kInfoStrLen];
};
static_assert(sizeof(ShimDeviceInfo) == kFullNameLen + 7 * kInfoStrLen, "layout");

struct ShimGrabResult {
    const void* context;
    void* h_buffer;
    const void* p_buffer;
    int32_t status;
    int32_t payload_type;
    int32_t pixel_type;
    uint64_t time_stamp;
    int32_t size_x, size_y, offset_x, offset_y, padding_x, padding_y;
    uint64_t payload_size;
    uint32_t error_code;
    uint64_t block_id;
};
static_assert(sizeof(ShimGrabResult) == 96, "layout");

// pylon.rs compares against the PylonC enum values; the C++ enums must agree.
static_assert(GrabStatus_Grabbed == 2, "grab status");
static_assert(PayloadType_Image == 0, "payload type");
static_assert(PixelType_Mono8 == 0x01080001, "pixel type");

constexpr int32_t kFail = static_cast<int32_t>(0xC2000000u);

thread_local std::string g_last_error;
std::mutex g_devices_mutex;
DeviceInfoList_t g_devices;  // the list built by the latest enumeration

template <class F>
int32_t guarded(F&& f) {
    try {
        f();
        return 0;
    } catch (const GenICam::GenericException& e) {
        g_last_error = e.GetDescription();
    } catch (const std::exception& e) {
        g_last_error = e.what();
    } catch (...) {
        g_last_error = "unknown pylon error";
    }
    return kFail;
}

void copy_str(char* dst, size_t cap, const String_t& s) {
    std::strncpy(dst, s.c_str(), cap - 1);
    dst[cap - 1] = '\0';
}

// Copy into a caller buffer of *len bytes (PylonC convention: NUL-terminated,
// *len updated to the bytes written including the NUL).
int32_t copy_out(const std::string& s, char* buf, size_t* len) {
    if (buf == nullptr || len == nullptr || *len == 0) return kFail;
    size_t n = std::min(s.size(), *len - 1);
    std::memcpy(buf, s.data(), n);
    buf[n] = '\0';
    *len = n + 1;
    return 0;
}

IPylonDevice* dev_of(void* h) { return static_cast<IPylonDevice*>(h); }
IStreamGrabber* stg_of(void* h) { return static_cast<IStreamGrabber*>(h); }

GenApi::INode* node(GenApi::INodeMap* map, const char* name) {
    GenApi::INode* n = map ? map->GetNode(name) : nullptr;
    if (n == nullptr) {
        throw RUNTIME_EXCEPTION("Node not existing: %s", name);
    }
    return n;
}

GenApi::INodeMap* dev_map(void* dev) { return dev_of(dev)->GetNodeMap(); }

}  // namespace

extern "C" {

int32_t PylonInitialize() {
    return guarded([] { Pylon::PylonInitialize(); });
}

int32_t PylonEnumerateDevices(size_t* num) {
    return guarded([&] {
        std::lock_guard<std::mutex> lock(g_devices_mutex);
        g_devices.clear();
        CTlFactory::GetInstance().EnumerateDevices(g_devices);
        *num = g_devices.size();
    });
}

int32_t PylonGetDeviceInfo(size_t index, ShimDeviceInfo* di) {
    return guarded([&] {
        std::lock_guard<std::mutex> lock(g_devices_mutex);
        if (index >= g_devices.size()) throw OUT_OF_RANGE_EXCEPTION("device index");
        const CDeviceInfo& d = g_devices[index];
        std::memset(di, 0, sizeof(*di));
        copy_str(di->full_name, kFullNameLen, d.GetFullName());
        copy_str(di->friendly_name, kInfoStrLen, d.GetFriendlyName());
        copy_str(di->vendor_name, kInfoStrLen, d.GetVendorName());
        copy_str(di->model_name, kInfoStrLen, d.GetModelName());
        copy_str(di->serial_number, kInfoStrLen, d.GetSerialNumber());
        copy_str(di->device_class, kInfoStrLen, d.GetDeviceClass());
        copy_str(di->device_version, kInfoStrLen, d.GetDeviceVersion());
        copy_str(di->user_defined_name, kInfoStrLen, d.GetUserDefinedName());
    });
}

int32_t PylonCreateDeviceByIndex(size_t index, void** dev) {
    return guarded([&] {
        std::lock_guard<std::mutex> lock(g_devices_mutex);
        if (index >= g_devices.size()) throw OUT_OF_RANGE_EXCEPTION("device index");
        *dev = CTlFactory::GetInstance().CreateDevice(g_devices[index]);
    });
}

int32_t PylonDestroyDevice(void* dev) {
    return guarded([&] { CTlFactory::GetInstance().DestroyDevice(dev_of(dev)); });
}

// pylon.rs passes PylonC's Control|Stream bit mask (1 | 2).
int32_t PylonDeviceOpen(void* dev, int32_t mode) {
    return guarded([&] {
        AccessModeSet set;
        if (mode & 1) set.set(Control);
        if (mode & 2) set.set(Stream);
        dev_of(dev)->Open(set);
    });
}

int32_t PylonDeviceClose(void* dev) {
    return guarded([&] { dev_of(dev)->Close(); });
}

int32_t PylonDeviceGetIntegerFeature(void* dev, const char* name, int64_t* v) {
    return guarded([&] {
        GenApi::CIntegerPtr p(node(dev_map(dev), name));
        if (!p.IsValid()) throw LOGICAL_ERROR_EXCEPTION("%s is not an integer", name);
        *v = p->GetValue();
    });
}

int32_t PylonDeviceGetFloatFeature(void* dev, const char* name, double* v) {
    return guarded([&] {
        GenApi::CFloatPtr p(node(dev_map(dev), name));
        if (!p.IsValid()) throw LOGICAL_ERROR_EXCEPTION("%s is not a float", name);
        *v = p->GetValue();
    });
}

int32_t PylonDeviceSetFloatFeature(void* dev, const char* name, double v) {
    return guarded([&] {
        GenApi::CFloatPtr p(node(dev_map(dev), name));
        if (!p.IsValid()) throw LOGICAL_ERROR_EXCEPTION("%s is not a float", name);
        p->SetValue(v);
    });
}

int32_t PylonDeviceFeatureFromString(void* dev, const char* name, const char* v) {
    return guarded([&] {
        GenApi::CValuePtr p(node(dev_map(dev), name));
        if (!p.IsValid()) throw LOGICAL_ERROR_EXCEPTION("%s has no value", name);
        p->FromString(v);
    });
}

int32_t PylonDeviceFeatureToString(void* dev, const char* name, char* buf, size_t* len) {
    std::string s;
    int32_t r = guarded([&] {
        GenApi::CValuePtr p(node(dev_map(dev), name));
        if (!p.IsValid()) throw LOGICAL_ERROR_EXCEPTION("%s has no value", name);
        s = p->ToString().c_str();
    });
    return r != 0 ? r : copy_out(s, buf, len);
}

int32_t PylonDeviceExecuteCommandFeature(void* dev, const char* name) {
    return guarded([&] {
        GenApi::CCommandPtr p(node(dev_map(dev), name));
        if (!p.IsValid()) throw LOGICAL_ERROR_EXCEPTION("%s is not a command", name);
        p->Execute();
    });
}

int32_t PylonDeviceGetStreamGrabber(void* dev, size_t index, void** stg) {
    return guarded([&] { *stg = dev_of(dev)->GetStreamGrabber(index); });
}

int32_t PylonStreamGrabberOpen(void* stg) {
    return guarded([&] { stg_of(stg)->Open(); });
}

int32_t PylonStreamGrabberClose(void* stg) {
    return guarded([&] { stg_of(stg)->Close(); });
}

int32_t PylonStreamGrabberGetWaitObject(void* stg, void** w) {
    return guarded([&] { *w = &stg_of(stg)->GetWaitObject(); });
}

// PylonC reads PayloadSize from the stream grabber when it has one (newer
// transport layers), otherwise from the camera.
int32_t PylonStreamGrabberGetPayloadSize(void* dev, void* stg, size_t* size) {
    return guarded([&] {
        GenApi::CIntegerPtr p(stg_of(stg)->GetNodeMap()->GetNode("PayloadSize"));
        if (!p.IsValid() || !GenApi::IsReadable(p)) {
            p = node(dev_map(dev), "PayloadSize");
        }
        if (!p.IsValid()) throw LOGICAL_ERROR_EXCEPTION("PayloadSize is not an integer");
        *size = static_cast<size_t>(p->GetValue());
    });
}

static void set_stream_int(void* stg, const char* name, size_t v) {
    GenApi::CIntegerPtr p(node(stg_of(stg)->GetNodeMap(), name));
    if (!p.IsValid()) throw LOGICAL_ERROR_EXCEPTION("%s is not an integer", name);
    p->SetValue(static_cast<int64_t>(v));
}

int32_t PylonStreamGrabberSetMaxNumBuffer(void* stg, size_t n) {
    return guarded([&] { set_stream_int(stg, "MaxNumBuffer", n); });
}

int32_t PylonStreamGrabberSetMaxBufferSize(void* stg, size_t n) {
    return guarded([&] { set_stream_int(stg, "MaxBufferSize", n); });
}

int32_t PylonStreamGrabberPrepareGrab(void* stg) {
    return guarded([&] { stg_of(stg)->PrepareGrab(); });
}

int32_t PylonStreamGrabberFinishGrab(void* stg) {
    return guarded([&] { stg_of(stg)->FinishGrab(); });
}

int32_t PylonStreamGrabberRegisterBuffer(void* stg, void* buf, size_t len, void** h) {
    return guarded([&] { *h = stg_of(stg)->RegisterBuffer(buf, len); });
}

int32_t PylonStreamGrabberDeregisterBuffer(void* stg, void* h) {
    return guarded([&] { stg_of(stg)->DeregisterBuffer(h); });
}

int32_t PylonStreamGrabberQueueBuffer(void* stg, void* h, const void* ctx) {
    return guarded([&] { stg_of(stg)->QueueBuffer(h, ctx); });
}

int32_t PylonStreamGrabberRetrieveResult(void* stg, ShimGrabResult* r, bool* ready) {
    return guarded([&] {
        GrabResult g;
        *ready = stg_of(stg)->RetrieveResult(g);
        if (!*ready) return;
        r->context = g.Context();
        r->h_buffer = g.Handle();
        r->p_buffer = g.Buffer();
        r->status = g.Status();
        r->payload_type = g.GetPayloadType();
        r->pixel_type = g.GetPixelType();
        r->time_stamp = g.GetTimeStamp();
        r->size_x = g.GetSizeX();
        r->size_y = g.GetSizeY();
        r->offset_x = g.GetOffsetX();
        r->offset_y = g.GetOffsetY();
        r->padding_x = g.GetPaddingX();
        r->padding_y = g.GetPaddingY();
        r->payload_size = static_cast<uint64_t>(g.GetPayloadSize());
        r->error_code = g.GetErrorCode();
        r->block_id = g.GetBlockID();
    });
}

int32_t PylonStreamGrabberStartStreamingIfMandatory(void* stg) {
    return guarded([&] { stg_of(stg)->StartStreamingIfMandatory(); });
}

int32_t PylonStreamGrabberStopStreamingIfMandatory(void* stg) {
    return guarded([&] { stg_of(stg)->StopStreamingIfMandatory(); });
}

int32_t PylonStreamGrabberFlushBuffersToOutput(void* stg) {
    return guarded([&] { stg_of(stg)->FlushBuffersToOutput(); });
}

int32_t PylonWaitObjectWait(void* w, uint32_t timeout_ms, bool* ready) {
    return guarded([&] { *ready = static_cast<WaitObject*>(w)->Wait(timeout_ms); });
}

int32_t GenApiGetLastErrorMessage(char* buf, size_t* len) {
    return copy_out(g_last_error, buf, len);
}

// The C++ exception description already carries the detail.
int32_t GenApiGetLastErrorDetail(char* buf, size_t* len) {
    return copy_out(std::string(), buf, len);
}

}  // extern "C"
