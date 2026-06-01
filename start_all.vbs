Set WshShell = CreateObject("WScript.Shell")
' Run the FastAPI Backend
WshShell.Run "cmd /c cd C:\Projects\Family-Ledger\backend && python main.py", 0, False
' Run the Vite Frontend
WshShell.Run "cmd /c cd C:\Projects\Family-Ledger\frontend && npm run dev", 0, False

' Give Vite a moment to start up
WScript.Sleep 2000
