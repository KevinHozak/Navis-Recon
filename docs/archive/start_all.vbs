Set WshShell = CreateObject("WScript.Shell")
' Run the FastAPI Backend
WshShell.Run "cmd /c ""cd /d C:\Projects\Navis-Recon\backend && python main.py""", 0, False
' Run the Vite Frontend
WshShell.Run "cmd /c ""cd /d C:\Projects\Navis-Recon\frontend && npm run dev""", 0, False

' Give Vite a moment to start up
WScript.Sleep 2000
