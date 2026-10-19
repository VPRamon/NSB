# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Independent offline pixel-boundary angular reference, not an NSB runtime.

Requires healpy==1.20.1, scipy==1.18.1, numpy==2.5.4. HEALPix boundaries
are located by healpy lookup/bisection on Galactic z=sin(b); QUADPACK then
integrates each constant-pixel interval and azimuth, splitting the geometric
horizon explicitly. No Rust quadrature, Siderust HEALPix code or subpixel
midpoint sum is used. The homogeneous path uses SciPy exprel. Stored numerical
changes/QUADPACK estimates are diagnostics rather than rigorous error bounds.
"""
import json
from pathlib import Path
import numpy as np
import healpy as hp
import scipy
from scipy.integrate import quad
from scipy.special import exprel
from math import pi, sin, cos, sqrt, exp

assert (hp.__version__, scipy.__version__, np.__version__) == ('1.20.1','1.18.1','2.5.4')
B=np.array([[-0.5602196629267336,0.8238736703920825,-0.08594244879531471],[0.8281813625875709,0.5591432318306364,-0.03839891874240515],[-0.016418280447458466,0.09268776366203746,0.9955598518092615]])
OUT=np.array([0.5523029120714643,0.2344386770935643,0.7999999999999999])
R,A,SSA,ABS=0.1,0.03,0.9,0.02
TAU=R+A+ABS
MUO=OUT[2]
centers=np.array(hp.pix2vec(1,np.arange(12)))
# Choose explicit above/below/crossing parents using an independent dense scan.
x=np.array(hp.pix2vec(128,np.arange(hp.nside2npix(128))))
parents=hp.vec2pix(1,*x)
mu=(B@x)[2]
mins=np.array([mu[parents==i].min() for i in range(12)])
maxs=np.array([mu[parents==i].max() for i in range(12)])
above=int(np.argmax(mins)); below=int(np.argmin(maxs))
view_parent=int(hp.vec2pix(1,*(B.T@OUT)))
cross=int(np.argmin(abs((B@centers)[2])))


def evaluate(values,g,scan,area=False):
    # Candidate pixel/horizon breakpoints are found independently at every phi.
    def inner(phi):
        z=np.linspace(-1.,1.,scan+1)
        pix=hp.ang2pix(1,np.arccos(z),phi)
        bounds=[-1.,1.]
        for j in np.flatnonzero(pix[:-1]!=pix[1:]):
            lo,hi=z[j],z[j+1]
            left=int(pix[j])
            for _ in range(45):
                mid=(lo+hi)/2
                if hp.ang2pix(1,np.arccos(mid),phi)==left: lo=mid
                else: hi=mid
            bounds.append((lo+hi)/2)
        normal=B[2]
        a=normal[0]*cos(phi)+normal[1]*sin(phi)
        b=normal[2]
        bounds.append(-a*np.sign(b)/sqrt(a*a+b*b))
        bounds.sort()
        def kernel(t):
            c=sqrt(max(0.,1-t*t))
            local=B@np.array([c*cos(phi),c*sin(phi),t])
            local/=np.linalg.norm(local)
            mui=local[2]
            if mui<=0: return 0.
            if area: return 1.
            cosine=float(local@OUT)
            pr=3*(1+cosine*cosine)/(16*pi)
            pa=(1-g*g)/(4*pi*(1+g*g-2*g*cosine)**1.5)
            aa,bb=TAU/mui,TAU/MUO
            path=exp(-min(aa,bb))*float(exprel(-abs(aa-bb)))/MUO
            return (R*pr+A*SSA*pa)*path
        total=0.
        for lo,hi in zip(bounds,bounds[1:]):
            pixel=int(hp.ang2pix(1,np.arccos((lo+hi)/2),phi))
            if values[pixel] and hi>lo:
                total+=values[pixel]*quad(kernel,lo,hi,epsabs=2e-11,epsrel=2e-9,limit=150)[0]
        return total
    value=error=0.
    for i in range(8):
        v,e=quad(inner,i*pi/4,(i+1)*pi/4,epsabs=1e-10,epsrel=2e-8,limit=200)
        value+=v;error+=e
    return value,error

cases=[]
for name,pixel,g in [('above',above,0.65),('below',below,0.65),('horizon',cross,0.65),('forward',view_parent,0.85),('backward',view_parent,-0.65),('isotropic',cross,0.)]:
    values=np.zeros(12);values[pixel]=1.
    coarse,_=evaluate(values,g,64)
    fine,error=evaluate(values,g,128)
    print(name,pixel,g,fine,'scan-change',abs(fine-coarse),'quad-estimate',error,flush=True)
    visible_area,area_error=evaluate(values,g,128,area=True)
    cases.append(dict(name=name,pixel=pixel,g=g,single=fine,scan_change=abs(fine-coarse),quad_estimate=error,visible_area_sr=visible_area,area_quad_estimate=area_error))
values=1+0.8*centers[2]
coarse,_=evaluate(values,0.65,64)
fine,error=evaluate(values,0.65,128)
cases.append(dict(name='anisotropic',pixel=None,g=0.65,single=fine,scan_change=abs(fine-coarse),quad_estimate=error))
result=dict(schema='nsb-healpix-angular-independent-v1',healpy=hp.__version__,scipy=scipy.__version__,numpy=np.__version__,nside=1,frame='Galactic RING',basis_local_to_galactic=B.tolist(),view_cartesian=OUT.tolist(),geometry=dict(longitude_deg=-111.59989,latitude_deg=31.96403,ellipsoid_height_m=2097.,jd_tt=2459680.8,jd_ut1=2459680.7992),optical_depths=dict(rayleigh=R,aerosol_extinction=A,aerosol_ssa=SSA,molecular_absorption=ABS),units='photon/(cm2 ns sr nm)',cases=cases)
path=Path('crates/nsb/tests/data/starlight-transport/angular-independent.json')
path.write_text(json.dumps(result,indent=2)+'\n')
