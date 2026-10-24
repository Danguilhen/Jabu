<?php
// Development router for PHP's built-in server. Serve existing files and
// send everything else to Movim's front controller.
$root = __DIR__ . '/movim/public';
if (!isset($_SERVER['QUERY_STRING'])) {
    $_SERVER['QUERY_STRING'] = '';
}
$path = parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH) ?? '/';
$file = realpath($root . $path);

if ($path !== '/' && $file !== false && is_file($file) && str_starts_with($file, $root)) {
    return false;
}

chdir($root);
require $root . '/index.php';
