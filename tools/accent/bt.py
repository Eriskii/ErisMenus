import lldb
OUT = open('/private/tmp/claude-501/-Users-eris-Documents-Programming-Projects/c62d994a-2835-49f1-8487-c1f881b93af1/scratchpad/accent/w/bt.txt', 'a')
def cb(frame, bp_loc, extra, internal_dict):
    t = frame.GetThread()
    lines = []
    for i in range(min(t.GetNumFrames(), 14)):
        f = t.GetFrameAtIndex(i)
        lines.append("  %s`%s" % (f.GetModule().GetFileSpec().GetFilename(), f.GetFunctionName()))
    OUT.write("BP %s\n%s\n" % (bp_loc.GetBreakpoint().GetID(), "\n".join(lines)))
    OUT.flush()
    return False
