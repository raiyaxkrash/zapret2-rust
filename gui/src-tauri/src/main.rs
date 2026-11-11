// В релизной сборке не показываем консольное окно.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    zapret2_gui_lib::run()
}
