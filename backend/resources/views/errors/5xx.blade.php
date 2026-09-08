@php
    $status = isset($exception) && method_exists($exception, 'getStatusCode')
        ? $exception->getStatusCode()
        : 500;
@endphp

@include('errors.page', [
    'status' => $status,
    'title' => 'Something went wrong',
    'description' => 'The service could not complete your request. Try again shortly.',
])
