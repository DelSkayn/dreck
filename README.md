
# Dreck
A experimental, mostly-safe, garbage collection library for rust build around zero cost abstractions.

A hard problem in GC library is the tracking of roots. The gc needs to know which pointers are considered alive.
In languages with builtin GC's like go and javascript the language itself keeps track of the roots by analyzing the program at compile time or by 
the use of a runtime. 
In the case of rust we need to do the work of keep track of roots ourself if we want to use a GC safely. The most use GC library does this 
by manually keep tracking of roots using considerable bookkeeping which might result in large overhead. This library tries to solve the problem of roots 
tracking by using rust's lifetimes to ensure roots are handled correctly.
