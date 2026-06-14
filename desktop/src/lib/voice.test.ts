import { describe, expect, it } from "vitest";
import { tuneOpus } from "./voice";

const OPUS_PARAMS = "minptime=10;useinbandfec=1;usedtx=1;stereo=0;sprop-stereo=0;maxaveragebitrate=64000;maxplaybackrate=48000";

describe("tuneOpus", () => {
  it("remplace une ligne fmtp Opus existante par les paramètres voix", () => {
    const sdp = [
      "m=audio 9 UDP/TLS/RTP/SAVPF 111",
      "a=rtpmap:111 opus/48000/2",
      "a=fmtp:111 minptime=10;useinbandfec=1",
      "a=rtpmap:0 PCMU/8000",
    ].join("\r\n");
    const out = tuneOpus(sdp);
    expect(out).toContain(`a=fmtp:111 ${OPUS_PARAMS}`);
    // FEC, DTX et mono explicitement présents.
    expect(out).toContain("useinbandfec=1");
    expect(out).toContain("usedtx=1");
    expect(out).toContain("stereo=0");
    // Ne touche pas aux autres codecs.
    expect(out).toContain("a=rtpmap:0 PCMU/8000");
  });

  it("insère une ligne fmtp si Opus n'en a pas", () => {
    const sdp = ["a=rtpmap:96 opus/48000/2", "a=rtpmap:0 PCMU/8000"].join("\r\n");
    const out = tuneOpus(sdp);
    expect(out).toContain(`a=fmtp:96 ${OPUS_PARAMS}`);
    // L'insertion suit immédiatement le rtpmap Opus.
    expect(out.indexOf("a=fmtp:96")).toBeGreaterThan(out.indexOf("a=rtpmap:96 opus"));
  });

  it("laisse le SDP intact s'il n'y a pas d'Opus", () => {
    const sdp = "a=rtpmap:0 PCMU/8000\r\na=rtpmap:8 PCMA/8000";
    expect(tuneOpus(sdp)).toBe(sdp);
  });

  it("cible le bon payload type dynamique", () => {
    const sdp = "a=rtpmap:120 opus/48000/2\r\na=fmtp:120 minptime=10";
    const out = tuneOpus(sdp);
    expect(out).toContain(`a=fmtp:120 ${OPUS_PARAMS}`);
    expect(out).not.toContain("a=fmtp:120 minptime=10\r\n");
  });
});
