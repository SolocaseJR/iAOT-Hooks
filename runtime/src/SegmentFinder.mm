#include <IAOT/SegmentFinder.hpp>
#include <cstring>
#if defined(__APPLE__)
namespace iaot {
const segment_command_64* FindSegment(const mach_header_64* h,const char* name){
    if(!h||!name||h->magic!=MH_MAGIC_64||h->sizeofcmds>128u*1024u*1024u) return nullptr;
    auto* p=reinterpret_cast<const std::uint8_t*>(h)+sizeof(*h); auto* end=p+h->sizeofcmds;
    for(std::uint32_t i=0;i<h->ncmds;++i){ if(p+sizeof(load_command)>end) return nullptr; auto* lc=reinterpret_cast<const load_command*>(p); if(lc->cmdsize<sizeof(load_command)||p+lc->cmdsize>end) return nullptr; if(lc->cmd==LC_SEGMENT_64&&lc->cmdsize>=sizeof(segment_command_64)){auto* s=reinterpret_cast<const segment_command_64*>(lc); if(std::strncmp(s->segname,name,sizeof(s->segname))==0)return s;} p+=lc->cmdsize; }
    return nullptr;
}
void* FindSegmentData(const mach_header_64* h,const char* name,std::uintptr_t slide,std::size_t* out){ if(out)*out=0; auto* s=FindSegment(h,name); if(!s)return nullptr; if(out)*out=static_cast<std::size_t>(s->vmsize); return reinterpret_cast<void*>(static_cast<std::uintptr_t>(s->vmaddr)+slide); }
}
#endif
