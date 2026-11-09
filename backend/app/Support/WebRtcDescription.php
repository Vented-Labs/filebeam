<?php

declare(strict_types=1);

namespace App\Support;

use Illuminate\Http\Request;

class WebRtcDescription
{
    /** @var list<string> */
    private const ATTRIBUTES = [
        'bundle-only', 'candidate', 'dcmap', 'dcsa', 'end-of-candidates', 'extmap',
        'extmap-allow-mixed', 'fingerprint', 'fmtp', 'group', 'ice-lite', 'ice-options',
        'ice-pwd', 'ice-ufrag', 'inactive', 'label', 'max-message-size', 'mid', 'msid',
        'msid-semantic', 'recvonly', 'rid', 'rtcp', 'rtcp-fb', 'rtcp-mux', 'rtcp-rsize',
        'rtpmap', 'sctp-port', 'sctpmap', 'sendonly', 'sendrecv', 'setup', 'simulcast',
        'ssrc', 'ssrc-group', 'tls-id',
    ];

    /** @return array{type: string, sdp: string} */
    public function from(Request $request, string $type): array
    {
        $contentLength = $request->header('Content-Length');
        $maximum = (int) config('filebeam.webrtc.max_sdp_bytes', 65_536);
        abort_if(is_string($contentLength) && (! ctype_digit($contentLength) || (int) $contentLength > $maximum + 512), 422, 'Invalid SDP.');
        $description = $request->validate(['description.type' => ['required', 'in:'.$type], 'description.sdp' => ['required', 'string']])['description'];
        abort_unless(is_array($description) && is_string($description['sdp']) && $this->valid($description['sdp'], $maximum), 422, 'Invalid SDP.');

        return ['type' => $type, 'sdp' => $description['sdp']];
    }

    private function valid(string $sdp, int $maximum): bool
    {
        if (strlen($sdp) > $maximum || preg_match('/\A[\x09\x0A\x0D\x20-\x7E]*\z/', $sdp) !== 1) {
            return false;
        }
        $lines = preg_split('/\r\n|\n|\r/', $sdp);
        if (! is_array($lines) || $lines === [] || $lines[0] !== 'v=0') {
            return false;
        }
        foreach ($lines as $line) {
            if ($line === '') {
                continue;
            }
            // SDP has a closed one-character field grammar; arbitrary text is not signaling.
            if (preg_match('/\A[vosiupecbtrzkam]=/D', $line) !== 1) {
                return false;
            }
            if (str_starts_with($line, 'a=')) {
                $attribute = explode(':', substr($line, 2), 2)[0];
                if (! in_array($attribute, self::ATTRIBUTES, true)) {
                    return false;
                }
            }
        }

        return true;
    }
}
