"""API compatible chat/completions. La clé reste dans le Gestionnaire d’identifiants Windows."""

import ctypes
import ctypes.wintypes as wt
import http.client
import json
import ssl
from urllib.parse import urlsplit


def credential(target):
    if not hasattr(ctypes, "WinDLL"):
        raise RuntimeError("Le Gestionnaire d’identifiants Windows est requis")

    class Credential(ctypes.Structure):
        _fields_ = [
            ("Flags", wt.DWORD),
            ("Type", wt.DWORD),
            ("TargetName", wt.LPWSTR),
            ("Comment", wt.LPWSTR),
            ("LastWritten", wt.FILETIME),
            ("CredentialBlobSize", wt.DWORD),
            ("CredentialBlob", ctypes.POINTER(ctypes.c_ubyte)),
            ("Persist", wt.DWORD),
            ("AttributeCount", wt.DWORD),
            ("Attributes", ctypes.c_void_p),
            ("TargetAlias", wt.LPWSTR),
            ("UserName", wt.LPWSTR),
        ]

    api = ctypes.WinDLL("Advapi32", use_last_error=True)
    ptr = ctypes.POINTER(Credential)()
    api.CredReadW.argtypes = (
        wt.LPCWSTR,
        wt.DWORD,
        wt.DWORD,
        ctypes.POINTER(ctypes.POINTER(Credential)),
    )
    api.CredFree.argtypes = (ctypes.c_void_p,)
    if not api.CredReadW(target, 1, 0, ctypes.byref(ptr)):
        raise ValueError(
            f"Ajoutez un identifiant générique Windows nommé {target}, avec votre clé API comme mot de passe"
        )
    try:
        return ctypes.string_at(
            ptr.contents.CredentialBlob, ptr.contents.CredentialBlobSize
        ).decode("utf-16le")
    finally:
        api.CredFree(ptr)


class Remote:
    def __init__(self, settings, consent):
        if consent is not True:
            raise ValueError("Envoi externe non autorisé")
        self.url = urlsplit(settings["endpoint"].strip())
        if (
            self.url.scheme != "https"
            or not self.url.hostname
            or self.url.username
            or self.url.password
            or self.url.query
            or self.url.fragment
        ):
            raise ValueError(
                "Une URL HTTPS sans clé, identifiants ou paramètres est requise"
            )
        self.model = settings["remote_model"].strip()
        if not self.model:
            raise ValueError("Indiquez le nom du modèle distant")
        try:
            self.token = credential("Rikai/" + self.url.netloc)
        except ValueError as err:
            try:
                self.token = credential("MeetingNotes/" + self.url.netloc)
            except ValueError:
                raise err from None

    def tokenize(self, text):
        # Une borne prudente du nombre de tokens pour une API dont le tokenizer est inconnu.
        return text

    def create_chat_completion(self, **params):
        con = http.client.HTTPSConnection(
            self.url.hostname,
            self.url.port,
            timeout=90,
            context=ssl.create_default_context(),
        )
        try:
            con.request(
                "POST",
                self.url.path.rstrip("/") + "/chat/completions",
                json.dumps(dict(model=self.model, **params)).encode(),
                {
                    "Content-Type": "application/json",
                    "Authorization": "Bearer " + self.token,
                },
            )
            response = con.getresponse()
            if response.status != 200:
                raise RuntimeError(f"Le fournisseur a répondu HTTP {response.status}")
            data = response.read(4_194_305)
            if len(data) > 4_194_304:
                raise ValueError("Réponse du fournisseur trop volumineuse")
            return json.loads(data)
        finally:
            con.close()
