"""Use the native Windows credential UI and the per-user credential vault."""

import ctypes
from ctypes import wintypes as wt


class Info(ctypes.Structure):
    _fields_ = [
        ("size", wt.DWORD),
        ("owner", wt.HWND),
        ("message", wt.LPCWSTR),
        ("caption", wt.LPCWSTR),
        ("banner", wt.HBITMAP),
    ]


class Entry(ctypes.Structure):
    _fields_ = [
        ("flags", wt.DWORD),
        ("kind", wt.DWORD),
        ("target", wt.LPWSTR),
        ("comment", wt.LPWSTR),
        ("changed", wt.FILETIME),
        ("size", wt.DWORD),
        ("blob", ctypes.c_void_p),
        ("persist", wt.DWORD),
        ("count", wt.DWORD),
        ("attributes", ctypes.c_void_p),
        ("alias", wt.LPWSTR),
        ("username", wt.LPWSTR),
    ]


PROMPT = {
    "fr": "Collez le jeton GitHub en lecture seule dans le champ du mot de passe.",
    "en": "Paste your read-only GitHub token in the password field.",
    "es": "Pega tu token de GitHub de solo lectura en el campo de contraseña.",
    "ru": "Вставьте токен GitHub с правом чтения в поле пароля.",
    "ja": "パスワード欄に読み取り専用の GitHub トークンを貼り付けてください。",
    "zh": "请在密码框中粘贴只读 GitHub 令牌。",
}


def prompt(locale):
    import webbrowser

    webbrowser.open(
        "https://github.com/settings/personal-access-tokens/new?name=Rikai&target_name=itsKushh&contents=read"
    )
    info = Info(
        ctypes.sizeof(Info),
        None,
        PROMPT.get(locale, PROMPT["en"]),
        "Rikai · GitHub",
        None,
    )
    user, secret = (
        ctypes.create_unicode_buffer("GitHub", 514),
        ctypes.create_unicode_buffer(256),
    )
    remember = wt.BOOL(False)
    fn = ctypes.windll.credui.CredUIPromptForCredentialsW
    fn.argtypes = [
        ctypes.POINTER(Info),
        wt.LPCWSTR,
        ctypes.c_void_p,
        wt.DWORD,
        wt.LPWSTR,
        wt.ULONG,
        wt.LPWSTR,
        wt.ULONG,
        ctypes.POINTER(wt.BOOL),
        wt.DWORD,
    ]
    try:
        status = fn(
            ctypes.byref(info),
            "Rikai/GitHub",
            None,
            0,
            user,
            len(user),
            secret,
            len(secret),
            ctypes.byref(remember),
            0x40000 | 0x80 | 0x2,
        )
        if status == 1223:
            return None
        if status:
            raise ctypes.WinError(status)
        return secret.value.strip()
    finally:
        ctypes.memset(ctypes.addressof(secret), 0, ctypes.sizeof(secret))


def save(target, text):
    raw = text.encode("utf-16le")
    buf = ctypes.create_string_buffer(raw)
    entry = Entry(
        kind=1,
        target=target,
        size=len(raw),
        blob=ctypes.addressof(buf),
        persist=2,
        username="GitHub",
    )
    api = ctypes.WinDLL("Advapi32", use_last_error=True)
    api.CredWriteW.argtypes = (ctypes.POINTER(Entry), wt.DWORD)
    try:
        if not api.CredWriteW(ctypes.byref(entry), 0):
            raise ctypes.WinError(ctypes.get_last_error())
    finally:
        ctypes.memset(ctypes.addressof(buf), 0, ctypes.sizeof(buf))
