<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0 Transitional//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd">
<html xmlns="http://www.w3.org/1999/xhtml" lang="{{ str_replace('_', '-', app()->getLocale()) }}">
<head>
<title>{{ config('filebeam.branding.name') }}</title>
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<meta http-equiv="Content-Type" content="text/html; charset=UTF-8" />
<meta name="color-scheme" content="light" />
<meta name="supported-color-schemes" content="light" />
<style>
@media only screen and (max-width: 600px) {
.inner-body,
.footer {
width: 100% !important;
}

.content-cell {
padding: 28px 22px !important;
}
}

@media only screen and (max-width: 500px) {
.button {
box-sizing: border-box !important;
text-align: center !important;
width: 100% !important;
}
}
</style>
{!! $head ?? '' !!}
</head>
<body style="background-color: #0b0914;">
<table class="wrapper" width="100%" cellpadding="0" cellspacing="0" role="presentation" bgcolor="#0b0914">
<tr>
<td align="center">
<table class="content" width="100%" cellpadding="0" cellspacing="0" role="presentation">
{!! $header ?? '' !!}

<tr>
<td class="body" width="100%" cellpadding="0" cellspacing="0" style="border: hidden !important;" bgcolor="#0b0914">
<table class="inner-body" align="center" width="600" cellpadding="0" cellspacing="0" role="presentation" bgcolor="#ffffff">
<tr>
<td class="content-cell">
{!! Illuminate\Mail\Markdown::parse($slot) !!}

{!! $subcopy ?? '' !!}
</td>
</tr>
</table>
</td>
</tr>

{!! $footer ?? '' !!}
</table>
</td>
</tr>
</table>
</body>
</html>
