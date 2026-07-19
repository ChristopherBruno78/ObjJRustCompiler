@implementation CPObject {
	
	id isa; 
	
}

+ (void)load
{
}

+ (void)initialize
{
//    CPLog("calling initialize "+self.name);
}

/*!
    Allocates a new instance of the receiver, and sends it an \c -init
    @return the new object
*/
+ (id)new
{
    return [[self alloc] init];
}

/*!
    Allocates a new instance of the receiving class
*/
+ (id)alloc
{
//    CPLog("calling alloc on " + self.name + ".");
    return class_createInstance(self);
}

+ (id)allocWithCoder:(CPCoder)aCoder
{
    return [self alloc];
}

/*!
    Initializes the receiver
    @return the initialized receiver
*/
- (id)init
{
    return self;
}

// Identifying classes
/*!
    Returns the Class object for this class definition.
*/
+ (Class)class
{
    return self;
}

/*!
    Returns the receiver's Class
*/
- (Class)class
{
    return isa;
}

/*!
    Returns the class object super class
*/
+ (Class)superclass
{
    return self.super_class;
}

/*!
    Returns \c YES if the receiving class is a subclass of \c aClass.
    @param aClass the class to test inheritance from
*/
+ (BOOL)isSubclassOfClass:(Class)aClass
{
    var theClass = self;

    for (; theClass; theClass = theClass.super_class)
        if (theClass === aClass)
            return YES;

    return NO;
}

/*!
    Returns \c YES if the receiver is a \c aClass type, or a subtype of it.
    @param aClass the class to test as the receiver's class or super class.
*/
- (BOOL)isKindOfClass:(Class)aClass
{
    return [isa isSubclassOfClass:aClass];
}

+ (BOOL)isKindOfClass:(Class)aClass
{
    return [self isSubclassOfClass:aClass];
}

/*!
    Returns \c YES if the receiver is of the \c aClass class type.
    @param aClass the class to test the receiver
*/
- (BOOL)isMemberOfClass:(Class)aClass
{
    return self.isa === aClass;
}

+ (BOOL)isMemberOfClass:(Class)aClass
{
    return self === aClass;
}

/*!
    Test whether instances of this class respond to the provided selector.
    @param aSelector the selector for which to test the class
    @return \c YES if instances of the class respond to the selector
*/
+ (BOOL)instancesRespondToSelector:(SEL)aSelector
{
    return !!class_getInstanceMethod(self, aSelector);
}

/*!
    Tests whether the receiver responds to the provided selector.
    @param aSelector the selector for which to test the receiver
    @return \c YES if the receiver responds to the selector
*/
- (BOOL)respondsToSelector:(SEL)aSelector
{
    // isa is isa.isa in class case.
    return !!class_getInstanceMethod(isa, aSelector);
}

/*!
    Tests if instances of a given class implement the provided selector regardless of inheritance.
    @param aSelector the selector for which to test the class
    @return \c YES if instances of the class implement the selector
*/
+ (BOOL)instancesImplementSelector:(SEL)aSelector
{
    var methods = class_copyMethodList(self),
        count = methods.length;

    while (count--)
        if (method_getName(methods[count]) === aSelector)
            return YES;

    return NO;
}

/*!
    Tests if the receiver implements the provided selector regardless of inheritance.
    @param aSelector the selector for which to test the receiver
    @return \c YES if the receiver implements the selector
*/
- (BOOL)implementsSelector:(SEL)aSelector
{
    return [[self class] instancesImplementSelector:aSelector];
}

/*!
    Test whether instances of this class conforms to the provided protocol.
    @param aProtocol the protocol for which to test the class
    @return \c YES if instances of the class conforms to the protocol
*/
+ (BOOL)conformsToProtocol:(Protocol)aProtocol
{
    return class_conformsToProtocol(self, aProtocol);
}

/*!
    Tests whether the receiver conforms to the provided protocol.
    @param protocol the protocol for which to test the class
    @return \c YES if instances of the class conforms to the protocol
*/
- (BOOL)conformsToProtocol:(Protocol)aProtocol
{
    return class_conformsToProtocol(isa, aProtocol);
}

// Obtaining method information

/*!
    Returns the implementation of the receiver's method for the provided selector.
    @param aSelector the selector for the method to return
    @return the method implementation ( a function )
*/
- (IMP)methodForSelector:(SEL)aSelector
{
    return class_getMethodImplementation(isa, aSelector);
}

/*!
    Returns the implementation of the receiving class' method for the provided selector.
    @param aSelector the selector for the class method to return
    @return the method implementation ( a function )
*/
+ (IMP)instanceMethodForSelector:(SEL)aSelector
{
    return class_getMethodImplementation(self, aSelector);
}


// Describing objects
/*!
    Returns a human readable string describing the receiver
*/
- (CPString)description
{
    return "<" + class_getName(isa) + " 0x" + [CPString stringWithHash:[self UID]] + ">";
}

+ (CPString)description
{
    return class_getName(self.isa);
}

// Sending Messages
/*!
    Sends the specified message to the receiver.
    @param aSelector the message to send
    @return the return value of the message
*/
- (id)performSelector:(SEL)aSelector
{
    return self.isa.objj_msgSend0(self, aSelector);
}

/*!
    Sends the specified message to the receiver, with one argument.
    @param aSelector the message to send
    @param anObject the message argument
    @return the return value of the message
*/
- (id)performSelector:(SEL)aSelector withObject:(id)anObject
{
    return self.isa.objj_msgSend1(self, aSelector, anObject);
}

/*!
    Sends the specified message to the receiver, with two arguments.
    @param aSelector the message to send
    @param anObject the first message argument
    @param anotherObject the second message argument
    @return the return value of the message
*/
- (id)performSelector:(SEL)aSelector withObject:(id)anObject withObject:(id)anotherObject
{
    return self.isa.objj_msgSend2(self, aSelector, anObject, anotherObject);
}

/*!
    Sends the specified message to the reciever, with any number of arguments.
    @param aSelector the message to send
    @param anObject... comma seperated objects to pass to the selector
    @return the return value of the message
*/
- (id)performSelector:(SEL)aSelector withObjects:(id)anObject, ...
{
    var params = [self, aSelector].concat(Array.prototype.slice.apply(arguments, [3]));
    return objj_msgSend.apply(this, params);
}


@end