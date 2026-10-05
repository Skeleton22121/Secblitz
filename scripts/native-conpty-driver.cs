// Validation-only Windows ConPTY host. It runs the frozen guide or an exact,
// reviewed mock/UI test; it never supplies repair/update commands or credentials.
using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public sealed class NativeConPty : IDisposable {
    [StructLayout(LayoutKind.Sequential)] struct Coord { public short X,Y; public Coord(short x,short y){X=x;Y=y;} }
    [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct Startup {
        public int cb; public string reserved,desktop,title; public int x,y,xsize,ysize,xchars,ychars,fill,flags;
        public short show,reserved2; public IntPtr reservedPtr,input,output,error;
    }
    [StructLayout(LayoutKind.Sequential)] struct StartupEx { public Startup startup; public IntPtr attributes; }
    [StructLayout(LayoutKind.Sequential)] struct ProcessInfo { public IntPtr process,thread; public uint pid,tid; }
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool CreatePipe(out IntPtr read,out IntPtr write,IntPtr attributes,uint size);
    [DllImport("kernel32.dll")] static extern int CreatePseudoConsole(Coord size,IntPtr input,IntPtr output,uint flags,out IntPtr console);
    [DllImport("kernel32.dll")] static extern int ResizePseudoConsole(IntPtr console,Coord size);
    [DllImport("kernel32.dll")] static extern void ClosePseudoConsole(IntPtr console);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool InitializeProcThreadAttributeList(IntPtr list,int count,int flags,ref IntPtr size);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool UpdateProcThreadAttribute(IntPtr list,uint flags,IntPtr attribute,IntPtr value,IntPtr size,IntPtr previous,IntPtr returned);
    [DllImport("kernel32.dll")] static extern void DeleteProcThreadAttributeList(IntPtr list);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcessW(string app,StringBuilder command,IntPtr processAttributes,IntPtr threadAttributes,bool inherit,uint flags,IntPtr environment,string cwd,ref StartupEx startup,out ProcessInfo info);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr h,uint ms);
    [DllImport("kernel32.dll")] static extern bool GetExitCodeProcess(IntPtr h,out uint code);
    [DllImport("kernel32.dll")] static extern bool TerminateProcess(IntPtr h,uint code);
    readonly object gate=new object();
    readonly MemoryStream bytes=new MemoryStream();
    FileStream input,output; Thread pump; IntPtr console,process; bool disposed;
    public uint Pid {get;private set;}
    public short Width {get;private set;}
    public short Height {get;private set;}
    static void Check(bool ok){if(!ok)throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());}
    public NativeConPty(string exe,string arguments,short width,short height,bool noColor) {
        string root=exe.StartsWith(@"C:\Windows\Temp\SecblitzV060UiFinal\",StringComparison.OrdinalIgnoreCase)?@"C:\Windows\Temp\SecblitzV060UiFinal\":@"C:\Windows\Temp\SecblitzV060CandidateValidation\";
        if(!exe.StartsWith(root,StringComparison.OrdinalIgnoreCase))throw new Exception("Only frozen validation binaries permitted");
        if(Path.GetFileName(exe)!="secblitz.exe" && Path.GetFileName(exe)!="native-cli.exe" && Path.GetFileName(exe)!="console-probe.exe")throw new Exception("Unexpected image");
        Width=width;Height=height;
        IntPtr ir,iw,orr,ow; Check(CreatePipe(out ir,out iw,IntPtr.Zero,0)); Check(CreatePipe(out orr,out ow,IntPtr.Zero,0));
        int hr=CreatePseudoConsole(new Coord(width,height),ir,ow,0,out console);
        CloseHandle(ir);CloseHandle(ow); if(hr!=0)Marshal.ThrowExceptionForHR(hr);
        input=new FileStream(new SafeFileHandle(iw,true),FileAccess.Write,4096,false);
        output=new FileStream(new SafeFileHandle(orr,true),FileAccess.Read,4096,false);
        IntPtr count=IntPtr.Zero; InitializeProcThreadAttributeList(IntPtr.Zero,1,0,ref count);
        IntPtr attrs=Marshal.AllocHGlobal(count);
        string oldColor=Environment.GetEnvironmentVariable("NO_COLOR"),oldTerm=Environment.GetEnvironmentVariable("TERM");
        try {
            Check(InitializeProcThreadAttributeList(attrs,1,0,ref count));
            Check(UpdateProcThreadAttribute(attrs,0,(IntPtr)0x00020016,console,(IntPtr)IntPtr.Size,IntPtr.Zero,IntPtr.Zero));
            StartupEx si=new StartupEx();si.startup.cb=Marshal.SizeOf(typeof(StartupEx));si.attributes=attrs;
            ProcessInfo info;
            Environment.SetEnvironmentVariable("TERM","xterm-256color");
            Environment.SetEnvironmentVariable("NO_COLOR",noColor?"1":null);
            // GuestControl's PowerShell host has redirected standard handles.
            // Open the new pseudoconsole's own devices explicitly for the child
            // instead of allowing those host pipes to become its stdio.
            string cmd=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System),"cmd.exe");
            string launcher=Path.Combine(root,"console-launcher.exe");
            string line="\""+cmd+"\" /d /s /c \"\""+launcher+"\" \""+exe+"\" "+arguments+" <CONIN$ >CONOUT$ 2>&1\"";
            Check(CreateProcessW(cmd,new StringBuilder(line),IntPtr.Zero,IntPtr.Zero,false,0x00080000,IntPtr.Zero,Path.GetDirectoryName(exe),ref si,out info));
            process=info.process;Pid=info.pid;CloseHandle(info.thread);
        } finally {
            Environment.SetEnvironmentVariable("NO_COLOR",oldColor);Environment.SetEnvironmentVariable("TERM",oldTerm);
            DeleteProcThreadAttributeList(attrs);Marshal.FreeHGlobal(attrs);
        }
        pump=new Thread(delegate(){
            byte[] buffer=new byte[16384];
            try {int n;while((n=output.Read(buffer,0,buffer.Length))>0){lock(gate){if(bytes.Length+n>16*1024*1024)throw new Exception("ConPTY output cap");bytes.Write(buffer,0,n);}}}
            catch(IOException){} catch(ObjectDisposedException){}
        });pump.IsBackground=true;pump.Start();
    }
    public int Mark(){lock(gate){return (int)bytes.Length;}}
    public byte[] Data(){lock(gate){return bytes.ToArray();}}
    public string Text(){return Encoding.UTF8.GetString(Data());}
    public void Key(string value){
        // ConPTY requests Win32-input-mode (?9001h). A lone legacy ESC can wait
        // indefinitely for the rest of a VT sequence; send an explicit key pair.
        if(value=="\x1b")value="\x1b[27;1;27;1;0;1_\x1b[27;1;27;0;0;1_";
        if(value=="\x1b[6~")value="\x1b[34;81;0;1;256;1_\x1b[34;81;0;0;256;1_";
        if(value=="\x1b[5~")value="\x1b[33;73;0;1;256;1_\x1b[33;73;0;0;256;1_";
        byte[] b=Encoding.UTF8.GetBytes(value);input.Write(b,0,b.Length);input.Flush();Thread.Sleep(180);
    }
    public void Resize(short width,short height){int hr=ResizePseudoConsole(console,new Coord(width,height));if(hr!=0)Marshal.ThrowExceptionForHR(hr);Width=width;Height=height;Thread.Sleep(400);}
    public void ResizeNow(short width,short height){int hr=ResizePseudoConsole(console,new Coord(width,height));if(hr!=0)Marshal.ThrowExceptionForHR(hr);Width=width;Height=height;}
    public void VirtualKey(int vk,int scan,int unicode,int control) {
        // Exactly one KEY_EVENT keydown and one keyup, each repeat count one.
        string pair="\x1b["+vk+";"+scan+";"+unicode+";1;"+control+";1_"+
            "\x1b["+vk+";"+scan+";"+unicode+";0;"+control+";1_";
        byte[] data=Encoding.UTF8.GetBytes(pair);input.Write(data,0,data.Length);input.Flush();Thread.Sleep(180);
    }
    public string WaitAny(string[] tokens,int after,int seconds) {
        Stopwatch watch=Stopwatch.StartNew();
        while(watch.Elapsed.TotalSeconds<seconds){
            byte[] all=Data();string text=Encoding.UTF8.GetString(all,Math.Min(after,all.Length),Math.Max(0,all.Length-after));
            foreach(string token in tokens)if(text.Contains(token))return token;
            if(WaitForSingleObject(process,0)==0)throw new Exception("Child exited before expected page: "+string.Join(" / ",tokens));
            Thread.Sleep(50);
        }
        throw new TimeoutException("ConPTY page timeout: "+string.Join(" / ",tokens));
    }
    public uint Finish(int seconds){if(WaitForSingleObject(process,(uint)seconds*1000)!=0)throw new TimeoutException("ConPTY child did not exit");Thread.Sleep(300);uint code;Check(GetExitCodeProcess(process,out code));return code;}
    public void Dump(string path){Thread.Sleep(250);File.WriteAllBytes(path,Data());}
    public void Dispose(){
        if(disposed)return;disposed=true;
        // Only this driver's exact frozen read-only guide or mock UI child can
        // be here. Failure cleanup never targets any servicing process.
        if(process!=IntPtr.Zero && WaitForSingleObject(process,0)!=0){TerminateProcess(process,99);WaitForSingleObject(process,5000);}
        if(console!=IntPtr.Zero)ClosePseudoConsole(console);
        if(input!=null)input.Dispose();if(output!=null)output.Dispose();
        if(process!=IntPtr.Zero)CloseHandle(process);
    }
}
