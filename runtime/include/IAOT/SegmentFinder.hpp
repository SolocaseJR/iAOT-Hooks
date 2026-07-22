#pragma once
#include <cstddef>
#include <cstdint>
#if defined(__APPLE__)
#include <mach-o/loader.h>
#else
struct mach_header_64;
struct segment_command_64;
#endif
namespace iaot {
const segment_command_64 *FindSegment(const mach_header_64 *header,
                                      const char *name);
void *FindSegmentData(const mach_header_64 *header, const char *name,
                      std::uintptr_t slide, std::size_t *outSize);
} // namespace iaot
