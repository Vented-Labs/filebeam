<x-mail::message>
@if (! empty($greeting))
# {{ $greeting }}
@elseif ($level === 'error')
# @lang('Whoops!')
@else
# @lang('Hello!')
@endif

@foreach ($introLines as $line)
{{ $line }}

@endforeach

@isset($actionText)
<?php
    $color = match ($level) {
        'success', 'error' => $level,
        default => 'primary',
    };
?>
<x-mail::button :url="$actionUrl" :color="$color">
{{ $actionText }}
</x-mail::button>
@endisset

@foreach ($outroLines as $line)
{{ $line }}

@endforeach

@if (! empty($salutation))
{{ $salutation }}
@else
@lang('Regards,')<br>
{{ config('filebeam.branding.name') }}
@endif

@isset($actionText)
<x-slot:subcopy>
@lang(
    "If you're having trouble clicking the \":actionText\" button, copy and paste the URL below\n".
    'into your web browser:',
    [
        'actionText' => $actionText,
    ]
) <span class="break-all">[{{ $displayableActionUrl }}]({{ $actionUrl }})</span>
</x-slot:subcopy>
@endisset
</x-mail::message>
