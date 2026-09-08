@php
    $status = isset($exception) && method_exists($exception, 'getStatusCode')
        ? $exception->getStatusCode()
        : 400;
@endphp

@include('errors.page', [
    'status' => $status,
    'title' => 'Request could not be completed',
    'description' => 'The service could not complete this request. Check the address or return to the start.',
])
