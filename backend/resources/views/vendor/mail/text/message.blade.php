<x-mail::layout>
    <x-slot:header>
        <x-mail::header :url="config('app.url')">
            {{ config('filebeam.branding.name') }}
        </x-mail::header>
    </x-slot:header>

    {{ $slot }}

    @isset($subcopy)
        <x-slot:subcopy>
            <x-mail::subcopy>
                {{ $subcopy }}
            </x-mail::subcopy>
        </x-slot:subcopy>
    @endisset

    <x-slot:footer>
        <x-mail::footer>
            © {{ config('filebeam.branding.copyright_year') }} {{ config('filebeam.branding.copyright_holder') }}. @lang('All rights reserved.')
        </x-mail::footer>
    </x-slot:footer>
</x-mail::layout>
