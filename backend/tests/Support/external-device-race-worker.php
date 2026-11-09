<?php

declare(strict_types=1);

use App\Http\Controllers\DeviceController;
use App\Models\Device;
use App\Models\DeviceCredential;
use App\Models\User;
use App\Services\DevicePairRendezvous;
use App\Services\DeviceRegistry;
use Illuminate\Contracts\Console\Kernel;
use Illuminate\Http\Request;
use Illuminate\Support\Facades\DB;

require dirname(__DIR__, 2).'/vendor/autoload.php';

[$script, $mode, $encodedPayload, $barrier] = array_pad($argv, 4, null);
assert(is_string($mode) && is_string($encodedPayload) && is_string($barrier));
$payload = json_decode(base64_decode($encodedPayload, true) ?: '', true, flags: JSON_THROW_ON_ERROR);
assert(is_array($payload));

$app = require dirname(__DIR__, 2).'/bootstrap/app.php';
$app->make(Kernel::class)->bootstrap();
$driver = getenv('FILEBEAM_EXTERNAL_DB_CONNECTION');
$url = getenv('FILEBEAM_EXTERNAL_DB_URL');
assert(is_string($driver) && is_string($url));
$config = config("database.connections.{$driver}");
assert(is_array($config));
$config['url'] = $url;
$config['database'] = null;
config(['database.connections.external_device_race' => $config, 'database.default' => 'external_device_race']);
DB::purge('external_device_race');

touch($barrier.'/'.getmypid().'.ready');
$deadline = microtime(true) + 15;
while (! is_file($barrier.'/release') && microtime(true) < $deadline) {
    usleep(10_000);
}
if (! is_file($barrier.'/release')) {
    throw new RuntimeException('External device race start barrier timed out.');
}

try {
    $controller = app(DeviceController::class);
    if ($mode === 'first-enrollment') {
        $user = User::query()->findOrFail($payload['user_id']);
        $device = Device::query()->findOrFail($payload['device_id']);
        $request = Request::create('/api/native/v1/devices/enrollments/'.$device->id, 'POST', ['challenge' => $payload['challenge'], 'signature' => $payload['signature']]);
        $request->setUserResolver(fn (): User => $user);
        $response = $controller->enroll($request, $device, app(DeviceRegistry::class));
    } elseif ($mode === 'renew') {
        $credential = DeviceCredential::query()->findOrFail($payload['credential_id']);
        $request = Request::create('/api/native/v1/devices/credentials/rotate', 'POST');
        $request->headers->set('Idempotency-Key', $payload['idempotency_key']);
        $request->attributes->set('device_credential', $credential);
        $response = $controller->rotateCredential($request, app(DeviceRegistry::class));
    } elseif ($mode === 'revoke') {
        $user = User::query()->findOrFail($payload['user_id']);
        $device = Device::query()->findOrFail($payload['device_id']);
        $request = Request::create('/api/native/v1/devices/'.$device->id, 'DELETE', ['current_password' => 'password']);
        $request->setUserResolver(fn (): User => $user);
        $response = $controller->revoke($request, $device, app(DeviceRegistry::class), app(DevicePairRendezvous::class));
    } else {
        throw new RuntimeException('Unknown external device race mode.');
    }

    fwrite(STDOUT, json_encode(['status' => $response->getStatusCode(), 'error' => null], JSON_THROW_ON_ERROR));
} catch (Throwable $exception) {
    fwrite(STDOUT, json_encode(['status' => method_exists($exception, 'getStatusCode') ? $exception->getStatusCode() : 500, 'error' => $exception::class], JSON_THROW_ON_ERROR));
}
