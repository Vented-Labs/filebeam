#!/usr/bin/env php
<?php
declare(strict_types=1);
if ($argc !== 3) { fwrite(STDERR,"Usage: desktop-update-index.php CURRENT RELEASE|--refresh\n"); exit(64); }
$current=is_file($argv[1])?json_decode((string)file_get_contents($argv[1]),true,512,JSON_THROW_ON_ERROR):['generation'=>0,'releases'=>[]];
if (($current['product'] ?? 'desktop') !== 'desktop') throw new RuntimeException('Invalid desktop index.');
if ($argv[2] === '--refresh') {
    $releases = $current['releases'] ?? [];
} else {
    $release=json_decode((string)file_get_contents($argv[2]),true,512,JSON_THROW_ON_ERROR);
    if (($release['product']??null)!=='desktop'||!is_array($release['assets']??null)) throw new RuntimeException('Invalid desktop release metadata.');
    $releases=array_values(array_filter($current['releases']??[],fn($r)=>($r['tag']??null)!==$release['tag'])); $releases[]=$release; usort($releases,fn($a,$b)=>version_compare($b['version'],$a['version']));
}
echo json_encode(['schema'=>1,'product'=>'desktop','generation'=>(int)($current['generation']??0)+1,'published_at'=>gmdate('Y-m-d\TH:i:s\Z'),'expires_at'=>gmdate('Y-m-d\TH:i:s\Z',time()+604800),'releases'=>$releases],JSON_UNESCAPED_SLASHES|JSON_PRETTY_PRINT)."\n";
