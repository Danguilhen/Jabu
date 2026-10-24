<?php

/*
 * Generate the Galener Jingle fixtures from the reference implementation.
 *
 * The live capture (capture-stanzas.py) needs a real Galene media negotiation
 * to produce session-accept/transport-info. Galene 1.0 now uses renegotiation
 * instead of server-initiated offers, so the content-add/content-remove stanzas
 * are generated here with the same Movim classes Galener uses at runtime.
 *
 * Run from dev/reference-testbed/movim:
 *   nix-shell ../../dev/reference-testbed/shell.nix --run \
 *     'php ../generate-fixtures.php ../../../crates/xmpp-core/tests/golden/galener'
 */

require __DIR__ . '/movim/vendor/autoload.php';

use App\User;
use Movim\Librairies\SDPtoJingle;
use Moxl\Stanza\Jingle;

$out = $argv[1] ?? (__DIR__ . '/../crates/xmpp-core/tests/golden/galener');
if (! is_dir($out)) {
    mkdir($out, 0777, true);
}

$room = 'voice@conference.localhost';
$sfu = sha1($room) . '@sfu.example.org';
$client = 'alice@localhost/desktop';
$participant = 'bob@localhost';
$sid = 'a1b2c3d4e5f60718293a4b5c6d7e8f90';

function fingerprint(): string
{
    $bytes = [];
    for ($i = 0; $i < 32; $i++) {
        $bytes[] = sprintf('%02X', random_int(0, 255));
    }

    return implode(':', $bytes);
}

function audioSdp(): string
{
    return implode("\r\n", [
        'v=0',
        'o=- 4611731400430051336 2 IN IP4 127.0.0.1',
        's=-',
        't=0 0',
        'a=group:BUNDLE 0',
        'a=msid-semantic: WMS stream',
        'm=audio 9 UDP/TLS/RTP/SAVPF 111 0 8 126',
        'c=IN IP4 0.0.0.0',
        'a=rtcp:9 IN IP4 0.0.0.0',
        'a=ice-ufrag:ufragalice',
        'a=ice-pwd:abcdefghijklmnopqrstuvwxyz',
        'a=ice-options:trickle',
        'a=fingerprint:sha-256 ' . fingerprint(),
        'a=setup:actpass',
        'a=mid:0',
        'a=sendonly',
        'a=rtcp-mux',
        'a=rtpmap:111 opus/48000/2',
        'a=rtcp-fb:111 transport-cc',
        'a=fmtp:111 minptime=10;useinbandfec=1',
        'a=rtpmap:0 PCMU/8000',
        'a=rtpmap:8 PCMA/8000',
        'a=rtpmap:126 telephone-event/8000',
        'a=msid:stream audio',
        'a=ssrc:123456789 cname:galener-fixture',
        'a=ssrc:123456789 msid:stream audio',
        '',
    ]);
}

function videoSdp(): string
{
    return implode("\r\n", [
        'v=0',
        'o=- 4611731400430051336 3 IN IP4 127.0.0.1',
        's=-',
        't=0 0',
        'a=group:BUNDLE 0',
        'a=msid-semantic: WMS screen',
        'm=video 9 UDP/TLS/RTP/SAVPF 96',
        'c=IN IP4 0.0.0.0',
        'a=rtcp:9 IN IP4 0.0.0.0',
        'a=ice-ufrag:ufragbob',
        'a=ice-pwd:zyxwvutsrqponmlkjihgfedcba',
        'a=ice-options:trickle',
        'a=fingerprint:sha-256 ' . fingerprint(),
        'a=setup:actpass',
        'a=mid:0',
        'a=sendonly',
        'a=rtcp-mux',
        'a=rtpmap:96 VP8/90000',
        'a=rtcp-fb:96 nack',
        'a=rtcp-fb:96 nack pli',
        'a=msid:screen video',
        'a=ssrc:987654321 cname:galener-fixture',
        'a=ssrc:987654321 msid:screen video',
        '',
    ]);
}

function iq(string $from, string $to, string $id, \DOMNode $payload): string
{
    $dom = new \DOMDocument('1.0', encoding: 'UTF-8');
    $iq = $dom->createElement('iq');
    $dom->appendChild($iq);
    $iq->setAttribute('type', 'set');
    $iq->setAttribute('id', $id);
    $iq->setAttribute('from', $from);
    $iq->setAttribute('to', $to);
    $iq->appendChild($dom->importNode($payload, true));

    return $dom->saveXML($iq);
}

function write(string $path, string $xml): void
{
    file_put_contents($path, $xml . "\n");
    echo "wrote $path\n";
}

function user(string $id): User
{
    $user = new User(['id' => $id]);
    $user->setRelation('session', null);
    $user->setRelation('capability', null);

    return $user;
}

$user = user($sfu);

$contentAdd = (new SDPtoJingle(
    user: $user,
    sdp: audioSdp(),
    sid: $sid,
    responder: $client,
    action: 'content-add',
    jingleParticipant: $participant,
    contentCategory: 'speaker'
))->generate();
write("$out/content_add_audio.xml", iq($sfu, $client, 'content-add-audio', $contentAdd));

$slides = (new SDPtoJingle(
    user: $user,
    sdp: videoSdp(),
    sid: $sid . '-slides',
    responder: $client,
    action: 'content-add',
    jingleParticipant: $participant,
    contentCategory: 'slides'
))->generate();
write("$out/content_add_slides.xml", iq($sfu, $client, 'content-add-slides', $slides));

$contentRemove = (new SDPtoJingle(
    user: $user,
    sdp: audioSdp(),
    sid: $sid,
    responder: $client,
    action: 'content-remove',
    jingleParticipant: $participant,
    contentCategory: 'speaker'
))->generate();
write("$out/content_remove_audio.xml", iq($sfu, $client, 'content-remove-audio', $contentRemove));

$contentAccept = (new SDPtoJingle(
    user: user($client),
    sdp: audioSdp(),
    sid: $sid,
    responder: $sfu,
    action: 'content-accept',
    jingleParticipant: $participant,
    contentCategory: 'speaker'
))->generate();
write("$out/content_accept_audio.xml", iq($client, $sfu, 'content-accept-audio', $contentAccept));

foreach (['' => 'mute', 'camera' => 'mute', 'screen' => 'unmute'] as $name => $action) {
    $jingle = $action === 'mute'
        ? Jingle::sessionMute(sid: $sid, name: $name)
        : Jingle::sessionUnmute(sid: $sid, name: $name);
    $participantNode = $jingle->ownerDocument->createElement('jingle-participant');
    $participantNode->setAttribute('xmlns', SDPtoJingle::JINGLE_PARTICIPANT_XMLNS);
    $participantNode->setAttribute('participant', $participant);
    $jingle->appendChild($participantNode);
    $suffix = trim($action . '_' . ($name === '' ? 'default' : $name));
    write("$out/session_info_$suffix.xml", iq($sfu, $client, 'session-info-' . $suffix, $jingle));
}
